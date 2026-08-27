//! Opening a directory in the configured IDE, detached from this process.
use crate::out;
use std::path::Path;
use std::process::{Command, Stdio};

/// `ide` is a command line (program + optional args); `path` is appended.
pub fn open_in_ide(ide: &str, path: &Path) {
    let mut words = ide.split_whitespace();
    let Some(program) = words.next() else {
        out::warn("no IDE configured (`ide` in .twig.toml is empty)");
        return;
    };
    let spawned = Command::new(program)
        .args(words)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(_) => out::say(format!("Opening in {program}: {}", path.display())),
        Err(_) => out::warn(format!("'{program}' not found in PATH; cannot open {}", path.display())),
    }
}
