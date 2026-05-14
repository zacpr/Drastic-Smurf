use std::process::Stdio;

use anyhow::{Context, Result};

use crate::core::cluster_manager::TunnelInfo;
use crate::core::config::ClusterConfig;

pub struct SshTunnel;

impl SshTunnel {
    pub async fn spawn(config: &ClusterConfig) -> Result<TunnelInfo> {
        let local_port = Self::find_free_port()?;

        let es_host = Self::parse_host(&config.host);
        let es_port = Self::parse_port(&config.host).unwrap_or(9200);

        let ssh_target = if config.ssh_user.is_empty() {
            config.ssh_host.clone()
        } else {
            format!("{}@{}", config.ssh_user, config.ssh_host)
        };

        let mut cmd = tokio::process::Command::new("ssh");
        cmd.arg("-N")
            .arg("-o")
            .arg("ServerAliveInterval=60")
            .arg("-o")
            .arg("ServerAliveCountMax=3")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-L")
            .arg(format!("127.0.0.1:{}:{}:{}", local_port, es_host, es_port))
            .arg("-p")
            .arg(config.ssh_port.to_string())
            .arg(&ssh_target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = cmd
            .spawn()
            .context("Failed to spawn SSH tunnel. Is 'ssh' installed and in PATH?")?;

        let pid = child.id().unwrap_or(0);

        // Detach the child so it keeps running independently
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        // Give the tunnel a moment to establish
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;

        Ok(TunnelInfo { local_port, pid })
    }

    pub fn kill_by_pid(pid: u32) {
        if pid == 0 {
            return;
        }
        #[cfg(unix)]
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .spawn();
        }
    }

    fn find_free_port() -> Result<u16> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .context("Failed to bind to find free port")?;
        let port = listener
            .local_addr()
            .context("Failed to get local address")?
            .port();
        drop(listener);
        Ok(port)
    }

    fn parse_host(host: &str) -> String {
        let host = host.trim();
        let host = host
            .strip_prefix("http://")
            .or_else(|| host.strip_prefix("https://"))
            .unwrap_or(host);
        host.split(':').next().unwrap_or(host).to_string()
    }

    fn parse_port(host: &str) -> Option<u16> {
        let host = host.trim();
        let host = host
            .strip_prefix("http://")
            .or_else(|| host.strip_prefix("https://"))
            .unwrap_or(host);
        host.split(':').nth(1)?.parse().ok()
    }
}
