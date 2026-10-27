//! Local crash log. Nothing in this file is uploaded.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub fn install() {
    let path = crash_log_path();
    std::panic::set_hook(Box::new(move |info| {
        let _ = append(&path, &info.to_string());
    }));
}

pub fn crash_log_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    base.join("Tracelyn").join("crash.log")
}

pub fn read_crash_log() -> String {
    let path = crash_log_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return String::new();
    };
    const MAX: usize = 64 * 1024;
    if text.len() <= MAX {
        text
    } else {
        let start = text.len() - MAX;
        let start = text.char_indices().find(|(i, _)| *i >= start).map(|(i, _)| i).unwrap_or(start);
        text[start..].to_string()
    }
}

fn append(path: &PathBuf, info: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "----- {when}")?;
    writeln!(file, "{info}")?;
    Ok(())
}
