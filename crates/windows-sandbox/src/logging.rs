//! Best-effort sandbox setup logging under `~/.zagens/.sandbox/`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub fn log_note(message: &str, log_base: Option<&Path>) {
    let Some(base) = log_base else {
        return;
    };
    if let Some(mut file) = log_writer(base) {
        let ts = chrono::Utc::now().to_rfc3339();
        let _ = writeln!(file, "[{ts}] {message}");
    }
}

pub fn log_writer(log_base: &Path) -> Option<File> {
    let _ = std::fs::create_dir_all(log_base);
    let path = log_base.join("setup.log");
    OpenOptions::new().create(true).append(true).open(path).ok()
}
