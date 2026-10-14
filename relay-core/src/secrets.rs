//! Secrets for local MCP servers live in Windows Credential Manager (D-047).
//!
//! Each value is stored under the target name `Relay MCP/<server>/<ENV_NAME>`. Off
//! Windows there is no store, and every call fails, so nothing is ever written to a
//! file in plain text.

use anyhow::Result;

pub const SERVICE: &str = "Relay MCP";

#[cfg(windows)]
fn entry(server: &str, key: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, &format!("{server}/{key}"))?)
}

#[cfg(windows)]
pub fn set(server: &str, key: &str, value: &str) -> Result<()> {
    entry(server, key)?.set_password(value)?;
    Ok(())
}

#[cfg(windows)]
pub fn get(server: &str, key: &str) -> Result<String> {
    Ok(entry(server, key)?.get_password()?)
}

#[cfg(windows)]
pub fn delete(server: &str, key: &str) -> Result<()> {
    entry(server, key)?.delete_credential()?;
    Ok(())
}

#[cfg(not(windows))]
pub fn set(_server: &str, _key: &str, _value: &str) -> Result<()> {
    anyhow::bail!("secret storage needs Windows Credential Manager")
}

#[cfg(not(windows))]
pub fn get(_server: &str, _key: &str) -> Result<String> {
    anyhow::bail!("secret storage needs Windows Credential Manager")
}

#[cfg(not(windows))]
pub fn delete(_server: &str, _key: &str) -> Result<()> {
    anyhow::bail!("secret storage needs Windows Credential Manager")
}
