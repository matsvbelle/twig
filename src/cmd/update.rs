//! `twig update`: replace this binary with the latest GitHub release (-c: only report it).
use crate::error::Result;
use crate::{bail, term::out};
use self_update::backends::github::{Update, UpdateBuilder};
use self_update::UpdateStrategy;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

const REPO_OWNER: &str = "matsvbelle";
const REPO_NAME: &str = "twig";
/// Releases ship only this static build, whatever triple this binary was compiled for.
const TARGET: &str = "x86_64-unknown-linux-musl";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn run(check: bool) -> Result<()> {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        bail!("Releases are only built for x86_64 Linux; update from source instead");
    }
    if check {
        return report();
    }
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or("Cannot locate the twig binary")?;
    let status = if is_writable(dir) {
        configure().build()?.update()?
    } else {
        let staging = tempfile::tempdir()?;
        let staged = staging.path().join("twig");
        let status = configure().bin_install_path(&staged).build()?.update()?;
        if status.is_updated() {
            out::say(format!("No write access to {}; installing with sudo...", dir.display()));
            let installed = Command::new("sudo").args(["install", "-m", "755"]).arg(&staged).arg(&exe).status();
            if !installed.is_ok_and(|s| s.success()) {
                bail!("sudo install failed; {} is unchanged", exe.display());
            }
        }
        status
    };
    if status.is_updated() {
        out::say(format!("{} twig {CURRENT} → {}", out::green("Updated:"), status.version()));
    } else {
        out::say(format!("twig {CURRENT} is up to date"));
    }
    Ok(())
}

fn report() -> Result<()> {
    match configure().build()?.is_update_available()? {
        Some(release) => out::say(format!(
            "{} twig {} (current {CURRENT}): run `twig update`",
            out::green("Update available:"),
            release.version()
        )),
        None => out::say(format!("twig {CURRENT} is up to date")),
    }
    Ok(())
}

/// self_update's own messages and Y/n prompt go to stdout, so all of them are off.
fn configure() -> UpdateBuilder {
    let mut b = Update::configure();
    b.repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("twig")
        .target(TARGET)
        .current_version(CURRENT)
        // The default prefers a semver-compatible release, i.e. for 0.x it stops short of a minor bump.
        .update_strategy(UpdateStrategy::Latest)
        .unattended();
    b
}

fn is_writable(dir: &Path) -> bool {
    let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else { return false };
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}
