//! `twig <branch> [base]`: create a worktree for the branch, or switch to it.
use crate::error::Result;
use crate::root::Root;
use crate::{color, git, hook, idea, launcher, out, tint};
use std::path::Path;
use std::process::Command;

/// Single-source-of-truth files linked from the main repo into each worktree.
const LINKED: [&str; 4] = ["venv", ".venv", ".gitlab_token", ".git-blame-ignore-revs"];

pub fn run(branch: &str, base: Option<&str>, open: bool) -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    let main = root.main_repo_at(&std::env::current_dir()?)?;
    let path = root.worktree_path(branch, &root.repo_name(&main));

    if path.exists() {
        out::say(format!("Worktree already exists: {}", path.display()));
        land(&root, &path, open);
        return Ok(());
    }

    checkout(&main, branch, base.unwrap_or("HEAD"), &path)?;

    if idea::setup(&main, &path)? {
        if let Some(t) = &root.config.tint {
            if let Err(e) = tint::apply(&path, branch, t) {
                out::warn(format!("  (background tint skipped: {e})"));
            }
        }
        if let Err(e) = color::apply(&main, &root.repos(), Some(&path)) {
            out::warn(format!("  (project color skipped: {e})"));
        }
        out::say("Set up .idea (shared config symlinked; build/state copied)");
    }
    link_shared(&main, &path)?;
    init_submodules(&main, &path)?;
    hook::install_branch_pin(&main)?;

    out::say("");
    out::say(format!("{} {}", out::green("Worktree ready:"), path.display()));
    land(&root, &path, open);
    Ok(())
}

/// Hand the path to the shell wrapper (cd), or open it in the IDE with `-o`.
pub fn land(root: &Root, path: &Path, open: bool) {
    if open {
        launcher::open_in_ide(&root.config.ide, path);
    } else {
        out::result(path);
    }
}

/// Existing local branch → check it out; on origin → track it; else new from base.
fn checkout(main: &Path, branch: &str, base: &str, path: &Path) -> Result<()> {
    let target = path.to_str().ok_or("non-UTF-8 worktree path")?;
    if git::ok(main, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]) {
        out::say(format!("Adding worktree on existing local branch '{branch}'..."));
        git::passthrough(main, &["worktree", "add", target, branch])
    } else if git::ok(main, &["ls-remote", "--exit-code", "--heads", "origin", branch]) {
        out::say(format!("Adding worktree tracking origin/{branch}..."));
        git::passthrough(main, &["fetch", "origin", branch])?;
        git::passthrough(main, &["worktree", "add", "--track", "-b", branch, target, &format!("origin/{branch}")])
    } else {
        out::say(format!("Creating new branch '{branch}' from {base}..."));
        git::passthrough(main, &["worktree", "add", "-b", branch, target, base])
    }
}

fn link_shared(main: &Path, wt: &Path) -> Result<()> {
    for item in LINKED {
        let src = main.join(item);
        let dst = wt.join(item);
        if src.exists() && !dst.exists() {
            std::os::unix::fs::symlink(&src, &dst)?;
            out::say(format!("Linked {item} -> main repo"));
        }
    }
    Ok(())
}

/// Seed the worktree's submodule object store from the main repo (local, CoW
/// where possible) so `submodule update` doesn't re-clone from the network.
fn init_submodules(main: &Path, wt: &Path) -> Result<()> {
    if !wt.join(".gitmodules").is_file() {
        return Ok(());
    }
    let main_modules = Path::new(&git::run(main, &["rev-parse", "--absolute-git-dir"])?).join("modules");
    let wt_modules = Path::new(&git::run(wt, &["rev-parse", "--absolute-git-dir"])?).join("modules");
    if main_modules.is_dir() && !wt_modules.exists() {
        out::say("Seeding submodules from main repo (local, no clone)...");
        let status = Command::new("cp").args(["-a", "--reflink=auto"]).arg(&main_modules).arg(&wt_modules).status()?;
        if !status.success() {
            out::warn("  (seeding failed; submodules will be cloned)");
        }
    }
    out::say("Initializing submodules...");
    git::passthrough(wt, &["submodule", "update", "--init", "--recursive"])
}
