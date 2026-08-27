//! `twig remove`: delete generated worktrees. The directory is removed directly
//! (worktrees with submodules can't be dropped by `git worktree remove`, and
//! Docker may leave root-owned files), then the parent repo's metadata pruned.
//! Branches are left intact.
use crate::error::Result;
use crate::root::{flatten_branch, name_of, Root, Worktree};
use crate::{bail, git, launcher, out};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

extern "C" {
    fn geteuid() -> u32;
}

pub fn run(target: Option<&str>, repo: Option<&str>, list: bool, open: bool) -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    if list {
        return list_worktrees(&root);
    }
    let remover = Remover::new(root, open);
    let Some(arg) = target else {
        let Some(top) = remover.current_top.clone() else {
            bail!("Not inside a git worktree. Pass a <path>/<branch>, or use --list.");
        };
        if !remover.root.is_generated(&top) {
            bail!("Current repo is not a generated worktree: {}\nPass a <path> or <branch> explicitly, or use --list.", top.display());
        }
        return remover.remove_all(&[top]);
    };
    if Path::new(arg).exists() {
        let path = Path::new(arg).canonicalize().map_err(|_| format!("Cannot resolve path: {arg}"))?;
        return remover.remove_all(&[path]);
    }
    let branch_dir = remover.root.worktrees_dir().join(flatten_branch(arg));
    if !branch_dir.is_dir() {
        bail!("No worktree path or branch '{arg}' found.");
    }
    let in_branch: Vec<Worktree> = remover.root.worktrees().into_iter().filter(|w| w.path.starts_with(&branch_dir)).collect();
    let targets: Vec<PathBuf> = match repo {
        Some(r) => {
            let main = remover.root.resolve_repo(r)?;
            let name = remover.root.repo_name(&main);
            match in_branch.iter().find(|w| w.repo == name) {
                Some(w) => vec![w.path.clone()],
                None => bail!("No worktree of {name} for '{arg}'."),
            }
        }
        None => in_branch.into_iter().map(|w| w.path).collect(),
    };
    remover.remove_all(&targets)
}

fn list_worktrees(root: &Root) -> Result<()> {
    let wts = root.worktrees();
    if wts.is_empty() {
        out::say(format!("No worktrees under {}", root.worktrees_dir().display()));
        return Ok(());
    }
    out::say(format!("Worktrees under {}:", root.worktrees_dir().display()));
    for w in wts {
        out::say(format!("  {:<40} [{}]", w.label(), w.branch().unwrap_or_default()));
    }
    Ok(())
}

pub struct Remover {
    pub root: Root,
    /// Top level of the checkout twig runs from, if any.
    pub current_top: Option<PathBuf>,
    /// When the current worktree is removed: open the main repo in the IDE
    /// (detached) instead of handing its path to the shell wrapper.
    pub open_main: bool,
}

impl Remover {
    pub fn new(root: Root, open_main: bool) -> Remover {
        let current_top = std::env::current_dir().ok().and_then(|d| git::toplevel(&d));
        Remover { root, current_top, open_main }
    }

    fn is_current(&self, path: &Path) -> bool {
        self.current_top.as_deref() == Some(path)
    }

    /// Remove all targets; the one we're inside goes LAST and, in open-main
    /// mode, detached (switching the IDE can close this very terminal).
    pub fn remove_all(&self, targets: &[PathBuf]) -> Result<()> {
        let mut failed = 0;
        for t in targets.iter().filter(|t| !self.is_current(t)) {
            if !self.remove_one(t) {
                failed += 1;
            }
        }
        if let Some(current) = targets.iter().find(|t| self.is_current(t)) {
            if !self.remove_one(current) {
                failed += 1;
            }
        }
        if failed > 0 {
            bail!("Done, with {failed} failure(s) — see messages above.");
        }
        Ok(())
    }

    /// Remove a single worktree (plus its branch dir if now empty). Removing
    /// the worktree we're in lands the user in its main repo.
    pub fn remove_one(&self, path: &Path) -> bool {
        let branch_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if self.is_current(path) {
            // Step out so we never delete our own cwd.
            let _ = std::env::set_current_dir(&self.root.dir);
            let main = worktree_at(path).parent().unwrap_or_default();
            if self.open_main {
                return detached_remove(path, &branch_dir, &main).is_ok();
            }
            let ok = remove_worktree(&self.root, path);
            self.root.cleanup_empty_dirs(&branch_dir);
            out::result(&main);
            return ok;
        }
        let ok = remove_worktree(&self.root, path);
        self.root.cleanup_empty_dirs(&branch_dir);
        ok
    }
}

fn worktree_at(path: &Path) -> Worktree {
    Worktree { path: path.to_path_buf(), branch_dir: String::new(), repo: name_of(path) }
}

/// Delete one generated worktree directory and prune its parent's metadata.
pub fn remove_worktree(root: &Root, wt: &Path) -> bool {
    if !root.is_generated(wt) {
        out::warn(format!("Refusing to remove '{}' (not a generated worktree).", wt.display()));
        return false;
    }
    if !wt.exists() {
        out::warn(format!("Skip: {} does not exist.", wt.display()));
        return false;
    }
    let parent = worktree_at(wt).parent();
    out::say(format!("Removing worktree: {}", wt.display()));

    // Rename aside FIRST (atomic): frees the path instantly even if the IDE holds
    // files open or is regenerating .idea, regardless of root-owned files inside.
    let trash = wt.with_extension(format!("trash.{}", std::process::id()));
    make_writable(wt);
    if fs::rename(wt, &trash).is_ok() {
        delete_tree(&trash);
    } else {
        delete_tree(wt);
    }

    // The IDE may recreate .idea at the original path (it saves on focus changes).
    // Keep clearing it; succeed once the path stayed gone for ~1s, cap at ~8s.
    let mut absent = 0;
    for _ in 0..80 {
        if wt.exists() {
            absent = 0;
            make_writable(wt);
            delete_tree(wt);
        } else {
            absent += 1;
            if absent >= 10 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if wt.exists() {
        out::warn(format!("  WARNING: {} keeps being recreated — close its IDE window, then:\n    rm -rf '{}'", wt.display(), wt.display()));
    }

    if let Some(parent) = parent.filter(|p| p.join(".git").exists()) {
        if git::ok(&parent, &["worktree", "prune"]) {
            out::say(format!("  Pruned worktree metadata in {}", name_of(&parent)));
        }
    }
    true
}

/// `rm -rf`, escalating to sudo only when foreign-owned (Docker root) files remain.
fn delete_tree(path: &Path) {
    let _ = fs::remove_dir_all(path);
    if path.exists() && has_foreign_files(path) {
        out::say("  Foreign-owned files (likely root from Docker); using sudo...");
        let _ = Command::new("sudo").args(["rm", "-rf"]).arg(path).status();
    }
}

fn has_foreign_files(path: &Path) -> bool {
    let me = unsafe { geteuid() };
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&p) else { continue };
        if meta.uid() != me {
            return true;
        }
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        }
    }
    false
}

/// `chmod -R u+w` so read-only trees (e.g. build outputs) can be deleted.
fn make_writable(path: &Path) {
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&p) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let mode = meta.permissions().mode();
        if mode & 0o200 == 0 {
            let _ = fs::set_permissions(&p, fs::Permissions::from_mode(mode | 0o200));
        }
        if meta.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        }
    }
}

/// Spawn `twig __bg-remove` in its own process group so it survives this
/// terminal closing (the IDE switching projects may close the worktree's terminal).
fn detached_remove(wt: &Path, branch_dir: &Path, main: &Path) -> Result<()> {
    let log = std::env::temp_dir().join(format!("twig-remove-{}.log", std::process::id()));
    let logfile = fs::File::create(&log)?;
    Command::new(std::env::current_exe()?)
        .arg("__bg-remove")
        .args([wt, branch_dir, main])
        .current_dir(wt.ancestors().nth(2).unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .stdout(Stdio::from(logfile.try_clone()?))
        .stderr(Stdio::from(logfile))
        .process_group(0)
        .spawn()?;
    out::say("Switching IDE to main repo; removing worktree in the background.");
    out::say(format!("  (progress/errors logged to {})", log.display()));
    Ok(())
}

/// The detached worker: switch the IDE to main, let it settle, then remove.
pub fn bg_remove(worktree: &str, branch_dir: &str, main: &str) -> Result<()> {
    let wt = Path::new(worktree);
    let root = Root::discover_from(wt)?;
    if !main.is_empty() {
        out::say(format!("Switching IDE to main repo: {main}"));
        launcher::open_in_ide(&root.config.ide, Path::new(main));
    }
    // Give the IDE time to close/deactivate the worktree project.
    std::thread::sleep(Duration::from_secs(3));
    remove_worktree(&root, wt);
    root.cleanup_empty_dirs(Path::new(branch_dir));
    Ok(())
}
