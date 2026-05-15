use anyhow::{Context, Result};
use keyring::Entry;

const APP_NAME: &str = "drastic-smurf";

pub fn set_password(cluster_name: &str, password: &str) -> Result<()> {
    let entry = Entry::new(APP_NAME, &format!("cluster:{}", cluster_name))?;
    entry
        .set_password(password)
        .context("Failed to store password in keyring")?;
    Ok(())
}

pub fn get_password(cluster_name: &str) -> Result<Option<String>> {
    let entry = Entry::new(APP_NAME, &format!("cluster:{}", cluster_name))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("Failed to retrieve password from keyring"),
    }
}

pub fn delete_password(cluster_name: &str) -> Result<()> {
    let entry = Entry::new(APP_NAME, &format!("cluster:{}", cluster_name))?;
    entry
        .delete_credential()
        .context("Failed to delete password from keyring")?;
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
