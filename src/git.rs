//! Thin wrapper over the `git` CLI. GIT_* variables are always cleared so a
//! twig invoked from a git alias or hook never inherits another repo's context.
use crate::error::Result;
use crate::bail;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const GIT_ENV: &[&str] = &[
    "GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_INDEX_FILE", "GIT_PREFIX", "GIT_OBJECT_DIRECTORY",
];

pub fn command(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd).args(args);
    for var in GIT_ENV {
        cmd.env_remove(var);
    }
    cmd
}

/// Run and return trimmed stdout; fails with git's stderr on non-zero exit.
pub fn run(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = command(cwd, args).stderr(Stdio::piped()).output()?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// Run quietly and only report success.
pub fn ok(cwd: &Path, args: &[&str]) -> bool {
    command(cwd, args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run with inherited stdio (user sees git's own progress output). In terminal
/// mode git's stdout goes to stderr so only twig's result line reaches stdout.
pub fn passthrough(cwd: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = command(cwd, args);
    if crate::out::terminal_mode() {
        cmd.stdout(Stdio::from(std::io::stderr()));
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

pub fn toplevel(cwd: &Path) -> Option<PathBuf> {
    run(cwd, &["rev-parse", "--show-toplevel"]).ok().map(PathBuf::from)
}

/// The MAIN repo of the checkout at `cwd`, also when `cwd` is a linked worktree.
pub fn main_repo(cwd: &Path) -> Option<PathBuf> {
    let common = run(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
    Path::new(&common).parent().map(Path::to_path_buf)
}

pub fn current_branch(cwd: &Path) -> Option<String> {
    run(cwd, &["branch", "--show-current"]).ok().filter(|b| !b.is_empty())
}
