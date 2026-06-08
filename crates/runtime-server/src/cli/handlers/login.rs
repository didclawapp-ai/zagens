use std::io::{self, IsTerminal, Read};

use anyhow::{Result, bail};

use crate::config::{clear_api_key, save_api_key};

pub fn run_login(api_key: Option<String>) -> Result<()> {
    let api_key = match api_key {
        Some(key) => key,
        None => read_api_key_from_stdin()?,
    };
    let saved = save_api_key(&api_key)?;
    println!("Saved API key to {}", saved.describe());
    Ok(())
}

pub fn run_logout() -> Result<()> {
    clear_api_key()?;
    println!("Cleared saved API key.");
    Ok(())
}

fn read_api_key_from_stdin() -> Result<String> {
    let mut stdin = io::stdin();
    if stdin.is_terminal() {
        bail!("No API key provided. Pass --api-key or pipe one via stdin.");
    }
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;
    let api_key = buffer.trim().to_string();
    if api_key.is_empty() {
        bail!("No API key provided via stdin.");
    }
    Ok(api_key)
}
