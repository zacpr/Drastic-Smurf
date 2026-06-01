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
            let mut guard = memory_store();
            let map = guard.get_or_insert_with(HashMap::new);
            map.insert(cluster_name.to_string(), password.to_string());
            return Ok(());
        }
    };
    match entry.set_password(password) {
        Ok(()) => {
            tracing::info!("Password saved to keyring for '{}'", cluster_name);
            let mut guard = memory_store();
            if let Some(map) = guard.as_mut() {
                map.insert(cluster_name.to_string(), password.to_string());
            }
            Ok(())
        }
        Err(err) => {
            tracing::warn!(
                "Keyring set_password failed for '{}': {}. Falling back to in-memory storage.",
                cluster_name,
                err
            );
            let mut guard = memory_store();
            let map = guard.get_or_insert_with(HashMap::new);
            map.insert(cluster_name.to_string(), password.to_string());
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
            let guard = memory_store();
            return Ok(guard.as_ref().and_then(|m| m.get(cluster_name).cloned()));
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
            let guard = memory_store();
            let result = guard.as_ref().and_then(|m| m.get(cluster_name).cloned());
            if result.is_some() {
                tracing::info!(
                    "get_password for '{}': found in memory fallback ({} chars)",
                    cluster_name,
                    result.as_ref().unwrap().len()
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
            let guard = memory_store();
            Ok(guard.as_ref().and_then(|m| m.get(cluster_name).cloned()))
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
    let mut guard = memory_store();
    if let Some(map) = guard.as_mut() {
        map.remove(cluster_name);
    }
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
