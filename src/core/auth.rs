use anyhow::{Context, Result};
use keyring::Entry;
use std::collections::HashMap;
use std::sync::Mutex;

const APP_NAME: &str = "drastic-smurf";

/// In-memory fallback for environments where the OS keyring is unavailable.
/// This is only used when keyring operations fail, so the app remains usable.
static MEMORY_KEYRING: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn memory_store() -> std::sync::MutexGuard<'static, Option<HashMap<String, String>>> {
    MEMORY_KEYRING.lock().unwrap_or_else(|e| e.into_inner())
}

fn mem_key(keyring_id: &str) -> String {
    format!("__mem__::{}", keyring_id)
}

fn mem_get(keyring_id: &str) -> Option<String> {
    let guard = memory_store();
    guard
        .as_ref()
        .and_then(|m| m.get(&mem_key(keyring_id)).cloned())
}

fn mem_set(keyring_id: &str, value: &str) {
    let mut guard = memory_store();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(mem_key(keyring_id), value.to_string());
}

fn mem_delete(keyring_id: &str) {
    let mut guard = memory_store();
    if let Some(map) = guard.as_mut() {
        map.remove(&mem_key(keyring_id));
    }
}

pub fn set_password(cluster_name: &str, password: &str) -> Result<()> {
    tracing::info!(
        "set_password called for '{}' ({} chars)",
        cluster_name,
        password.len()
    );
    let entry = match Entry::new(APP_NAME, &format!("cluster:{}", cluster_name)) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(
                "Keyring Entry::new failed for '{}': {}. Falling back to in-memory storage.",
                cluster_name,
                err
            );
            mem_set(&format!("cluster:{}", cluster_name), password);
            return Ok(());
        }
    };
    match entry.set_password(password) {
        Ok(()) => {
            tracing::info!("Password saved to keyring for '{}'", cluster_name);
            mem_set(&format!("cluster:{}", cluster_name), password);
            Ok(())
        }
        Err(err) => {
            tracing::warn!(
                "Keyring set_password failed for '{}': {}. Falling back to in-memory storage.",
                cluster_name,
                err
            );
            mem_set(&format!("cluster:{}", cluster_name), password);
            Ok(())
        }
    }
}

pub fn get_password(cluster_name: &str) -> Result<Option<String>> {
    let entry = match Entry::new(APP_NAME, &format!("cluster:{}", cluster_name)) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(
                "Keyring Entry::new failed for '{}': {}. Trying in-memory fallback.",
                cluster_name,
                err
            );
            return Ok(mem_get(&format!("cluster:{}", cluster_name)));
        }
    };
    match entry.get_password() {
        Ok(pw) => {
            tracing::info!(
                "get_password for '{}' from keyring: found ({} chars)",
                cluster_name,
                pw.len()
            );
            Ok(Some(pw))
        }
        Err(keyring::Error::NoEntry) => {
            tracing::info!(
                "get_password for '{}': NoEntry in keyring, checking memory fallback",
                cluster_name
            );
            let result = mem_get(&format!("cluster:{}", cluster_name));
            if let Some(ref pw) = result {
                tracing::info!(
                    "get_password for '{}': found in memory fallback ({} chars)",
                    cluster_name,
                    pw.len()
                );
            } else {
                tracing::warn!("get_password for '{}': NOT FOUND anywhere", cluster_name);
            }
            Ok(result)
        }
        Err(err) => {
            tracing::warn!(
                "Keyring get_password failed for '{}': {}. Trying in-memory fallback.",
                cluster_name,
                err
            );
            Ok(mem_get(&format!("cluster:{}", cluster_name)))
        }
    }
}

pub fn delete_password(cluster_name: &str) -> Result<()> {
    if let Ok(entry) = Entry::new(APP_NAME, &format!("cluster:{}", cluster_name)) {
        let _ = entry.delete_credential();
    } else {
        tracing::warn!(
            "Keyring Entry::new failed for '{}'. Skipping keyring delete, clearing memory fallback.",
            cluster_name
        );
    }
    mem_delete(&format!("cluster:{}", cluster_name));
    Ok(())
}

#[allow(dead_code)]
pub fn set_api_token(token_name: &str, token: &str) -> Result<()> {
    let entry = Entry::new(APP_NAME, &format!("token:{}", token_name))?;
    entry
        .set_password(token)
        .context("Failed to store API token in keyring")?;
    Ok(())
}

#[allow(dead_code)]
pub fn get_api_token(token_name: &str) -> Result<Option<String>> {
    let entry = Entry::new(APP_NAME, &format!("token:{}", token_name))?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("Failed to retrieve API token from keyring"),
    }
}

// --- LLM provider API keys -----------------------------------------------

fn llm_key_id(provider_id: &str) -> String {
    format!("llm:{}", provider_id)
}

pub fn set_llm_api_key(provider_id: &str, key: &str) -> Result<()> {
    let id = llm_key_id(provider_id);
    match Entry::new(APP_NAME, &id) {
        Ok(entry) => match entry.set_password(key) {
            Ok(()) => {
                mem_set(&id, key);
                Ok(())
            }
            Err(err) => {
                tracing::warn!(
                    "Keyring set failed for LLM provider '{}': {}. Using memory fallback.",
                    provider_id,
                    err
                );
                mem_set(&id, key);
                Ok(())
            }
        },
        Err(err) => {
            tracing::warn!(
                "Keyring Entry::new failed for LLM provider '{}': {}. Using memory fallback.",
                provider_id,
                err
            );
            mem_set(&id, key);
            Ok(())
        }
    }
}

pub fn get_llm_api_key(provider_id: &str) -> Result<Option<String>> {
    let id = llm_key_id(provider_id);
    match Entry::new(APP_NAME, &id) {
        Ok(entry) => match entry.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(mem_get(&id)),
            Err(e) => Err(e).context("Failed to retrieve LLM API key from keyring"),
        },
        Err(err) => {
            tracing::warn!(
                "Keyring Entry::new failed for LLM provider '{}': {}. Trying memory fallback.",
                provider_id,
                err
            );
            Ok(mem_get(&id))
        }
    }
}

pub fn delete_llm_api_key(provider_id: &str) -> Result<()> {
    let id = llm_key_id(provider_id);
    if let Ok(entry) = Entry::new(APP_NAME, &id) {
        let _ = entry.delete_credential();
    }
    mem_delete(&id);
    Ok(())
}
