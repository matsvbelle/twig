//! `twig init` / `twig status`.
use crate::cli::InitArgs;
use crate::config::{Config, Tint, CONFIG_FILE};
use crate::error::Result;
use crate::root::{find_config, Root, INACTIVE_MSG};
use crate::{bail, git, out};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(args: InitArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    if git::toplevel(&cwd).is_some() {
        bail!("Error: {} is inside a git repository. Init the folder that holds your repos instead.", cwd.display());
    }
    if let Some(dir) = find_config(&cwd) {
        if dir != cwd {
            bail!("Error: nested twig directories are not supported; {} is already twigged.", dir.display());
        }
        return update(&Root::load(&dir)?, &args);
    }
    if let Some(inner) = find_nested(&cwd) {
        bail!("Error: nested twig directories are not supported; {} is already twigged.", inner.display());
    }

    let config = Config {
        worktrees: args.name.clone(),
        ide: args.ide.clone().unwrap_or_else(|| crate::config::DEFAULT_IDE.to_string()),
        tint: tint_from(&args, Tint::default()),
    };
    let root = Root { dir: cwd.clone(), config };
    fs::create_dir_all(root.worktrees_dir())?;
    root.save()?;
    out::say(format!("twig active for {} ({} git repositories)", cwd.display(), root.repos().len()));
    out::say(format!("  worktrees folder: {}", root.worktrees_dir().display()));
    out::say(format!("  ide: {}", root.config.ide));
    out::say(format!("  config: {}", root.dir.join(CONFIG_FILE).display()));
    warn_shell_integration();
    Ok(())
}

/// Re-init on an active root: only the ide/tint options may change.
fn update(root: &Root, args: &InitArgs) -> Result<()> {
    let has_tint = args.no_tint || args.opacity.is_some() || args.saturation.is_some() || args.lightness.is_some();
    if !has_tint && args.ide.is_none() {
        bail!("twig is already active for {} (pass --ide or tint options to update them)", root.dir.display());
    }
    let mut root = root.clone();
    if let Some(ide) = &args.ide {
        root.config.ide = ide.clone();
    }
    if has_tint {
        root.config.tint = tint_from(args, root.config.tint.clone().unwrap_or_default());
    }
    root.save()?;
    out::say(format!("Updated {}", root.dir.join(CONFIG_FILE).display()));
    warn_shell_integration();
    Ok(())
}

fn tint_from(args: &InitArgs, base: Tint) -> Option<Tint> {
    if args.no_tint {
        return None;
    }
    Some(Tint {
        opacity: args.opacity.unwrap_or(base.opacity),
        saturation: args.saturation.unwrap_or(base.saturation),
        lightness: args.lightness.unwrap_or(base.lightness),
    })
}

/// Set by the `twig()` zsh function so the binary knows the wrapper is active.
pub fn shell_integration_active() -> bool {
    std::env::var_os("TWIG_SHELL").is_some()
}

pub fn warn_shell_integration() {
    if !shell_integration_active() {
        out::warn(out::yellow(
            "warning: shell integration not active (no cd / tab completion).\n  \
             Run scripts/install.sh or add  eval \"$(twig shell zsh)\"  to your .zshrc, then open a new shell.",
        ));
    }
}

/// First `.twig.toml` anywhere below `dir` (hidden dirs skipped).
pub fn find_nested(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.join(CONFIG_FILE).is_file() {
            return Some(path);
        }
        if let Some(found) = find_nested(&path) {
            return Some(found);
        }
    }
    None
}

pub fn status() -> Result<()> {
    let cwd = std::env::current_dir()?;
    match find_config(&cwd) {
        Some(dir) => {
            let root = Root::load(&dir)?;
            out::say(format!(
                "twig active for {} with {} git repositories, {} worktrees ({})",
                root.dir.display(),
                root.repos().len(),
                root.worktrees().len(),
                root.config.worktrees
            ));
            out::say(format!("  ide: {}", root.config.ide));
            match &root.config.tint {
                Some(t) => out::say(format!(
                    "  tint: opacity {}%, saturation {}, lightness {}",
                    t.opacity, t.saturation, t.lightness
                )),
                None => out::say("  tint: off"),
            }
            warn_shell_integration();
        }
        None => out::say(INACTIVE_MSG),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_scan_is_unbounded_and_skips_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a/b/c/d/e");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir_all(tmp.path().join(".git/x")).unwrap();
        fs::write(tmp.path().join(".git/x").join(CONFIG_FILE), "").unwrap();
        assert_eq!(find_nested(tmp.path()), None);
        fs::write(deep.join(CONFIG_FILE), "").unwrap();
        assert_eq!(find_nested(tmp.path()), Some(deep));
    }
}
