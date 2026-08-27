//! The branch-pin `reference-transaction` hook, installed once per main repo.
use crate::error::Result;
use crate::{git, out};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const HOOK: &str = include_str!("../hooks/branch-pin.sh");
/// Marker line identifying our hook, so re-installs overwrite only our own.
const MARKER: &str = "twig-branch-pin";

pub fn hooks_dir(main: &Path) -> Result<PathBuf> {
    if let Ok(custom) = git::run(main, &["config", "--get", "core.hooksPath"]) {
        if !custom.is_empty() {
            return Ok(main.join(custom));
        }
    }
    Ok(PathBuf::from(git::run(main, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?).join("hooks"))
}

pub fn install_branch_pin(main: &Path) -> Result<()> {
    let dir = hooks_dir(main)?;
    fs::create_dir_all(&dir)?;
    let hook = dir.join("reference-transaction");
    if let Ok(existing) = fs::read_to_string(&hook) {
        if !existing.contains(MARKER) {
            out::warn("Note: an unrelated reference-transaction hook exists; branch-pin not installed.");
            return Ok(());
        }
    }
    fs::write(&hook, HOOK)?;
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
    out::say(format!("Branch-pin hook ready: {}", hook.display()));
    Ok(())
}
