//! SSH local port forwarding (`ssh -N -L`) for clusters only reachable through an SSH host.
//!
//! The tunnel is transparent to TLS and HTTP: requests keep the cluster's real URL host, so
//! SNI, certificate checks and the `Host` header match a direct connection. Only the TCP
//! connection is redirected to the local end of the forward (see `EsClient`).

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{HOST, HeaderValue};
use tokio::io::AsyncBufReadExt;

use crate::core::config::ClusterConfig;

/// How long to wait for the local end of the forward to accept connections. Generous because
/// ssh may be waiting on an askpass dialog (key passphrase, unknown host key).
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// stderr lines kept per tunnel for diagnostics.
const LOG_CAPACITY: usize = 200;

/// The host and port the tunnel forwards to, taken from the cluster's URL. The SSH server
/// resolves and connects to it, so it must be reachable from there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelTarget {
    /// URL host as `Url::host_str` gives it: lowercase domain, IPv4, or bracketed IPv6.
    pub host: String,
    /// Explicit port, or the scheme's default (443 for https, 80 for http).
    pub port: u16,
    pub is_ip: bool,
}

impl TunnelTarget {
    /// Parses a cluster host the way `EsClient` builds request URLs: `http://` is assumed
    /// when no scheme is given.
    pub fn from_host(host: &str) -> Result<Self, String> {
        let host = host.trim();
        let with_scheme = if host.contains("://") {
            host.to_string()
        } else {
            format!("http://{}", host)
        };
        let url = reqwest::Url::parse(&with_scheme)
            .map_err(|e| format!("Invalid cluster host '{}': {}", host, e))?;
        let url_host = url
            .host_str()
            .ok_or_else(|| format!("Cluster host '{}' has no hostname", host))?
            .to_string();
        let port = url
            .port_or_known_default()
            .ok_or_else(|| format!("Cluster host '{}' has no port", host))?;
        Ok(Self {
            is_ip: url_host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .is_ok(),
            host: url_host,
            port,
        })
    }

    /// Whether a request URL is for this target (and not, say, the cluster's Kibana).
    pub fn matches(&self, url: &reqwest::Url) -> bool {
        url.host_str() == Some(self.host.as_str()) && url.port_or_known_default() == Some(self.port)
    }

    /// Points a request for this target at the local end of the tunnel. A domain stays in the
    /// URL (the client resolves it to 127.0.0.1); an IP is replaced, since there is nothing to
    /// resolve. The `Host` header keeps the original authority.
    pub fn redirect(&self, mut req: reqwest::Request, local_port: u16) -> reqwest::Request {
        let authority = match req.url().port() {
            Some(port) => format!("{}:{}", self.host, port),
            None => self.host.clone(),
        };
        if self.is_ip {
            let _ = req.url_mut().set_ip_host(IpAddr::V4(Ipv4Addr::LOCALHOST));
        }
        let _ = req.url_mut().set_port(Some(local_port));
        if !req.headers().contains_key(HOST)
            && let Ok(value) = HeaderValue::from_str(&authority)
        {
            req.headers_mut().insert(HOST, value);
        }
        req
    }

    /// The address the tunnel's HTTP client resolves this target's domain to.
    pub fn local_addr(local_port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), local_port)
    }
}

/// Why a tunnel could not be opened: a one-line summary for status displays, plus ssh's
/// stderr for the detailed connection test.
#[derive(Debug, Clone)]
pub struct TunnelError {
    pub summary: String,
    pub command: String,
    pub logs: Vec<String>,
}

/// A running `ssh -N -L` process. Dropping it kills ssh.
pub struct SshTunnel {
    pub local_port: u16,
    alive: Arc<AtomicBool>,
    logs: Arc<Mutex<VecDeque<String>>>,
    command: String,
    // Dropping the sender tells the supervisor task to kill ssh.
    _kill: tokio::sync::oneshot::Sender<()>,
}

impl SshTunnel {
    /// Starts ssh and waits until the forward accepts connections, ssh exits, or
    /// `READY_TIMEOUT` passes. `verbose` adds `-v` for the connection test's log.
    pub async fn open(
        config: &ClusterConfig,
        target: &TunnelTarget,
        verbose: bool,
    ) -> Result<Self, TunnelError> {
        let ssh_target = ssh_target(config);
        let local_port = find_free_port().map_err(|e| TunnelError {
            summary: format!("Could not find a free local port: {}", e),
            command: String::new(),
            logs: Vec::new(),
        })?;
        let args = ssh_args(config, target, local_port, verbose);
        let command = format!("ssh {}", args.join(" "));

        let mut cmd = tokio::process::Command::new("ssh");
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: don't flash a console window from the GUI app.
            cmd.creation_flags(0x0800_0000);
        }

        let mut child = cmd.spawn().map_err(|e| TunnelError {
            summary: format!(
                "Failed to start ssh: {}. Is the OpenSSH client installed and on PATH?",
                e
            ),
            command: command.clone(),
            logs: Vec::new(),
        })?;

        let logs = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAPACITY)));
        let stderr_done = Arc::new(tokio::sync::Notify::new());
        if let Some(stderr) = child.stderr.take() {
            let logs = logs.clone();
            let stderr_done = stderr_done.clone();
            tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    push_log(&logs, line);
                }
                stderr_done.notify_one();
            });
        } else {
            stderr_done.notify_one();
        }

        let alive = Arc::new(AtomicBool::new(true));
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
        {
            let alive = alive.clone();
            let logs = logs.clone();
            let name = config.name.clone();
            tokio::spawn(async move {
                tokio::select! {
                    status = child.wait() => {
                        let status = status
                            .map(|s| s.to_string())
                            .unwrap_or_else(|e| format!("unknown status ({})", e));
                        tracing::warn!("[{}] SSH tunnel process exited: {}", name, status);
                        push_log(&logs, format!("ssh exited: {}", status));
                    }
                    _ = kill_rx => {
                        let _ = child.kill().await;
                    }
                }
                alive.store(false, Ordering::Relaxed);
            });
        }

        let tunnel = Self {
            local_port,
            alive,
            logs,
            command,
            _kill: kill_tx,
        };

        let started = Instant::now();
        loop {
            if !tunnel.is_alive() {
                // Let the reader drain ssh's last words before diagnosing.
                let _ = tokio::time::timeout(Duration::from_secs(1), stderr_done.notified()).await;
                let logs = tunnel.logs();
                return Err(TunnelError {
                    summary: diagnose(&logs, config).unwrap_or_else(|| {
                        format!(
                            "ssh to {} exited before the tunnel was ready{}",
                            ssh_target,
                            last_error_line(&logs)
                                .map(|l| format!(": {}", l))
                                .unwrap_or_default()
                        )
                    }),
                    command: tunnel.command.clone(),
                    logs,
                });
            }
            let probe = tokio::time::timeout(
                Duration::from_secs(1),
                tokio::net::TcpStream::connect(TunnelTarget::local_addr(local_port)),
            )
            .await;
            if matches!(probe, Ok(Ok(_))) {
                tracing::info!(
                    "[{}] SSH tunnel ready: {} after {}ms",
                    config.name,
                    describe(config, target, local_port),
                    started.elapsed().as_millis()
                );
                return Ok(tunnel);
            }
            if started.elapsed() >= READY_TIMEOUT {
                let logs = tunnel.logs();
                return Err(TunnelError {
                    summary: diagnose(&logs, config).unwrap_or_else(|| {
                        format!(
                            "Timed out after {}s waiting for the SSH tunnel via {} \
                             (ssh may be waiting for a password or host key confirmation it \
                             cannot ask for)",
                            READY_TIMEOUT.as_secs(),
                            ssh_target
                        )
                    }),
                    command: tunnel.command.clone(),
                    logs,
                });
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    /// ssh's recent complaints (channel open failures and the like), for request errors.
    pub fn recent_errors(&self) -> Vec<String> {
        let logs = self.logs.lock().unwrap();
        logs.iter()
            .rev()
            .filter(|l| !l.starts_with("debug"))
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

pub fn ssh_target(config: &ClusterConfig) -> String {
    let user = config.ssh_user.trim();
    let host = config.ssh_host.trim();
    if user.is_empty() {
        host.to_string()
    } else {
        format!("{}@{}", user, host)
    }
}

/// `127.0.0.1:47123 → elastic.example.com:443 via user@bastion:22`
pub fn describe(config: &ClusterConfig, target: &TunnelTarget, local_port: u16) -> String {
    format!(
        "127.0.0.1:{} → {}:{} via {}:{}",
        local_port,
        target.host,
        target.port,
        ssh_target(config),
        config.ssh_port
    )
}

fn ssh_args(
    config: &ClusterConfig,
    target: &TunnelTarget,
    local_port: u16,
    verbose: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-N".into()];
    if verbose {
        args.push("-v".into());
    }
    for opt in [
        "ExitOnForwardFailure=yes",
        "ServerAliveInterval=30",
        "ServerAliveCountMax=3",
        "ConnectTimeout=10",
    ] {
        args.push("-o".into());
        args.push(opt.into());
    }
    args.push("-L".into());
    args.push(format!(
        "127.0.0.1:{}:{}:{}",
        local_port, target.host, target.port
    ));
    args.push("-p".into());
    args.push(config.ssh_port.to_string());
    args.push(ssh_target(config));
    args
}

fn push_log(logs: &Mutex<VecDeque<String>>, line: String) {
    let mut logs = logs.lock().unwrap();
    if logs.len() >= LOG_CAPACITY {
        logs.pop_front();
    }
    logs.push_back(line);
}

fn find_free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn last_error_line(logs: &[String]) -> Option<&str> {
    logs.iter().rev().map(|l| l.trim()).find(|l| {
        !l.is_empty()
            && !l.starts_with("debug")
            && !l.starts_with("ssh exited")
            && !l.starts_with('@')
    })
}

/// Turns ssh's stderr into an explanation of the usual failures.
fn diagnose(logs: &[String], config: &ClusterConfig) -> Option<String> {
    let has = |needle: &str| logs.iter().any(|l| l.contains(needle));
    let ssh_host = config.ssh_host.trim();
    let known_hosts_name = if config.ssh_port == 22 {
        ssh_host.to_string()
    } else {
        format!("'[{}]:{}'", ssh_host, config.ssh_port)
    };

    if has("forwarding disabled due to host key check failure")
        || has("Port forwarding is disabled to avoid man-in-the-middle")
        || has("REMOTE HOST IDENTIFICATION HAS CHANGED")
    {
        return Some(format!(
            "The SSH host key for {} does not match the one in your known_hosts file, so ssh \
             refused to forward ports. If the server was rebuilt or its key legitimately \
             changed, run `ssh-keygen -R {}` and then connect once with ssh to accept the new \
             key.",
            ssh_host, known_hosts_name
        ));
    }
    if has("Host key verification failed") {
        return Some(format!(
            "ssh could not verify the host key for {}. Connect once from a terminal \
             (ssh -p {} {}) to check and accept it.",
            ssh_host,
            config.ssh_port,
            ssh_target(config)
        ));
    }
    if has("Permission denied") {
        return Some(format!(
            "SSH authentication to {} failed. The app cannot type a password into ssh: use a \
             key loaded in ssh-agent (or an IdentityFile in ~/.ssh/config).",
            ssh_target(config)
        ));
    }
    if has("Could not resolve hostname") {
        return Some(format!("Could not resolve the SSH host '{}'.", ssh_host));
    }
    if has("Connection timed out") || has("Operation timed out") || has("No route to host") {
        return Some(format!(
            "Could not reach the SSH host {} on port {}. If it is only reachable through a jump \
             host, set ProxyJump for it in ~/.ssh/config (Host entries match the SSH Host \
             field exactly as typed).",
            ssh_host, config.ssh_port
        ));
    }
    if has("Connection refused") && !has("channel") {
        return Some(format!(
            "The SSH host {} refused the connection on port {}.",
            ssh_host, config.ssh_port
        ));
    }
    if has("Address already in use") {
        return Some("The local tunnel port was taken by another program; try again.".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tunnel_config() -> ClusterConfig {
        ClusterConfig {
            name: "c".into(),
            host: "https://elastic.example.com".into(),
            ssh_tunnel: true,
            ssh_host: "10.2.60.11".into(),
            ssh_user: "dante".into(),
            ..Default::default()
        }
    }

    #[test]
    fn target_uses_the_urls_own_port() {
        let t = TunnelTarget::from_host("https://Elastic.Example.com/").unwrap();
        assert_eq!(t.host, "elastic.example.com");
        assert_eq!(t.port, 443);
        assert!(!t.is_ip);

        assert_eq!(
            TunnelTarget::from_host("http://es:9200").unwrap().port,
            9200
        );
        assert_eq!(TunnelTarget::from_host("es-node:9200").unwrap().port, 9200);
        assert_eq!(TunnelTarget::from_host("http://es").unwrap().port, 80);

        let ip = TunnelTarget::from_host("https://10.0.0.5:9243").unwrap();
        assert!(ip.is_ip);
        let v6 = TunnelTarget::from_host("https://[::1]:9200").unwrap();
        assert_eq!(v6.host, "[::1]");
        assert!(v6.is_ip);

        assert!(TunnelTarget::from_host("").is_err());
    }

    #[test]
    fn matches_only_the_cluster_host() {
        let t = TunnelTarget::from_host("https://elastic.example.com").unwrap();
        let es = reqwest::Url::parse("https://elastic.example.com/_cluster/health").unwrap();
        let es_explicit = reqwest::Url::parse("https://elastic.example.com:443/").unwrap();
        let kibana = reqwest::Url::parse("https://kibana.example.com/api/status").unwrap();
        let other_port = reqwest::Url::parse("https://elastic.example.com:9200/").unwrap();
        assert!(t.matches(&es));
        assert!(t.matches(&es_explicit));
        assert!(!t.matches(&kibana));
        assert!(!t.matches(&other_port));
    }

    fn request(url: &str) -> reqwest::Request {
        reqwest::Request::new(reqwest::Method::GET, reqwest::Url::parse(url).unwrap())
    }

    #[test]
    fn redirect_keeps_the_hostname_and_host_header() {
        let t = TunnelTarget::from_host("https://elastic.example.com").unwrap();
        let req = t.redirect(request("https://elastic.example.com/_cat/nodes"), 47000);
        assert_eq!(
            req.url().as_str(),
            "https://elastic.example.com:47000/_cat/nodes"
        );
        assert_eq!(req.headers()[HOST], "elastic.example.com");

        let t = TunnelTarget::from_host("http://es:9200").unwrap();
        let req = t.redirect(request("http://es:9200/"), 47001);
        assert_eq!(req.url().as_str(), "http://es:47001/");
        assert_eq!(req.headers()[HOST], "es:9200");
    }

    #[test]
    fn redirect_replaces_an_ip_host() {
        let t = TunnelTarget::from_host("https://10.0.0.5:9243").unwrap();
        let req = t.redirect(request("https://10.0.0.5:9243/_cluster/health"), 47002);
        assert_eq!(
            req.url().as_str(),
            "https://127.0.0.1:47002/_cluster/health"
        );
        assert_eq!(req.headers()[HOST], "10.0.0.5:9243");
    }

    #[test]
    fn args_forward_to_the_target() {
        let config = tunnel_config();
        let target = TunnelTarget::from_host(&config.host).unwrap();
        let args = ssh_args(&config, &target, 47003, false);
        assert!(args.contains(&"127.0.0.1:47003:elastic.example.com:443".to_string()));
        assert_eq!(args.last().unwrap(), "dante@10.2.60.11");
        assert!(!args.contains(&"-v".to_string()));
    }

    #[test]
    fn diagnoses_a_changed_host_key() {
        let logs = vec![
            "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@".to_string(),
            "@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @".to_string(),
            "Port forwarding is disabled to avoid man-in-the-middle attacks.".to_string(),
            "Error: forwarding disabled due to host key check failure".to_string(),
        ];
        let mut config = tunnel_config();
        let msg = diagnose(&logs, &config).unwrap();
        assert!(msg.contains("ssh-keygen -R 10.2.60.11"), "{msg}");

        config.ssh_port = 2222;
        let msg = diagnose(&logs, &config).unwrap();
        assert!(msg.contains("ssh-keygen -R '[10.2.60.11]:2222'"), "{msg}");
    }

    #[test]
    fn diagnoses_auth_failure_and_falls_back_to_last_line() {
        let config = tunnel_config();
        let logs = vec!["dante@10.2.60.11: Permission denied (publickey).".to_string()];
        assert!(diagnose(&logs, &config).unwrap().contains("authentication"));

        let logs = vec![
            "debug1: something".to_string(),
            "kex_exchange_identification: read: Connection reset by peer".to_string(),
            "ssh exited: exit status: 255".to_string(),
        ];
        assert!(diagnose(&logs, &config).is_none());
        assert_eq!(
            last_error_line(&logs),
            Some("kex_exchange_identification: read: Connection reset by peer")
        );
    }
}
