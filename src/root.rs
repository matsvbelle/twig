//! The twigged directory: discovered by walking up from the cwd to the nearest
//! `.twig.toml`. Main repos live (possibly nested) under it; a repo is identified
//! by its path relative to the root, mirrored under `<worktrees>/<branch>/`.
use crate::config::{Config, CONFIG_FILE};
use crate::error::Result;
use crate::{bail, git};
use std::fs;
use std::path::{Path, PathBuf};

pub const INACTIVE_MSG: &str = "twig inactive, use 'twig init' to set up twig for the current directory";

#[derive(Debug, Clone)]
pub struct Root {
    pub dir: PathBuf,
    pub config: Config,
}

/// A generated worktree at `<root>/<worktrees>/<branch_dir>/<repo>/`, where
/// `repo` is the main repo's path relative to the root (e.g. `external/lib`).
#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch_dir: String,
    pub repo: String,
}

impl Worktree {
    pub fn label(&self) -> String {
        format!("{}/{}", self.branch_dir, self.repo)
    }

    /// The main repo, read from the `.git` file's gitdir pointer (works even
    /// when the worktree is broken, and before it is deleted).
    pub fn parent(&self) -> Option<PathBuf> {
        let pointer = fs::read_to_string(self.path.join(".git")).ok()?;
        let gitdir = pointer.strip_prefix("gitdir: ")?.trim();
        let main = gitdir.split("/.git/worktrees/").next()?;
        Some(PathBuf::from(main))
    }

    pub fn branch(&self) -> Option<String> {
        git::current_branch(&self.path)
    }
}

/// Nearest ancestor (inclusive) of `start` holding a `.twig.toml`.
pub fn find_config(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(Path::to_path_buf).find(|d| d.join(CONFIG_FILE).is_file())
}

impl Root {
    pub fn load(dir: &Path) -> Result<Root> {
        let text = fs::read_to_string(dir.join(CONFIG_FILE))?;
        Ok(Root { dir: dir.to_path_buf(), config: Config::parse(&text)? })
    }

    pub fn discover_from(start: &Path) -> Result<Root> {
        match find_config(start) {
            Some(dir) => Root::load(&dir),
            None => bail!("{INACTIVE_MSG}"),
        }
    }

    pub fn discover() -> Result<Root> {
        Root::discover_from(&std::env::current_dir()?)
    }

    pub fn save(&self) -> Result<()> {
        fs::write(self.dir.join(CONFIG_FILE), self.config.to_toml())?;
        Ok(())
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.dir.join(&self.config.worktrees)
    }

    /// Git repos anywhere under the root (not inside other repos, hidden dirs
    /// or the worktrees folder), sorted by path.
    pub fn repos(&self) -> Vec<PathBuf> {
        let mut repos = Vec::new();
        self.scan_repos(&self.dir, &mut repos);
        repos
    }

    fn scan_repos(&self, dir: &Path, out: &mut Vec<PathBuf>) {
        for d in sorted_dirs(dir) {
            if name_of(&d).starts_with('.') || d == self.worktrees_dir() {
                continue;
            }
            if d.join(".git").exists() {
                out.push(d);
            } else {
                self.scan_repos(&d, out);
            }
        }
    }

    /// Repo identity: its path relative to the root, as a string.
    pub fn repo_name(&self, main: &Path) -> String {
        main.strip_prefix(&self.dir).unwrap_or(main).to_string_lossy().into_owned()
    }

    pub fn worktree_path(&self, branch: &str, repo: &str) -> PathBuf {
        self.worktrees_dir().join(flatten_branch(branch)).join(repo)
    }

    /// All generated worktrees, sorted by branch dir then repo path.
    pub fn worktrees(&self) -> Vec<Worktree> {
        let mut out = Vec::new();
        for bdir in sorted_dirs(&self.worktrees_dir()) {
            Self::scan_worktrees(&bdir, &bdir, &mut out);
        }
        out
    }

    /// A worktree is the first directory below the branch dir holding a `.git`.
    fn scan_worktrees(bdir: &Path, dir: &Path, out: &mut Vec<Worktree>) {
        for d in sorted_dirs(dir) {
            if d.join(".git").exists() {
                out.push(Worktree {
                    path: d.clone(),
                    branch_dir: name_of(bdir),
                    repo: d.strip_prefix(bdir).unwrap().to_string_lossy().into_owned(),
                });
            } else {
                Self::scan_worktrees(bdir, &d, out);
            }
        }
    }

    /// Worktree dirs of `branch_dir` for `repo`; a bare name matches a nested
    /// repo only when no top-level repo has that name (see `resolve_repo`).
    pub fn resolve_repo(&self, name: &str) -> Result<PathBuf> {
        let repos = self.repos();
        let rel = |r: &PathBuf| self.repo_name(r);
        if let Some(r) = repos.iter().find(|r| rel(r) == name) {
            return Ok(r.clone());
        }
        let by_base: Vec<&PathBuf> = repos.iter().filter(|r| name_of(r) == name).collect();
        let by_sub: Vec<&PathBuf> = {
            let needle = name.to_lowercase();
            repos.iter().filter(|r| rel(r).to_lowercase().contains(&needle)).collect()
        };
        let hits = if by_base.is_empty() { by_sub } else { by_base };
        match hits.as_slice() {
            [one] => Ok((*one).clone()),
            [] => bail!("No repo matching '{name}' under {}.", self.dir.display()),
            many => {
                let names: Vec<String> = many.iter().map(|m| format!("  {}", rel(m))).collect();
                bail!("Ambiguous '{name}' — matches:\n{}", names.join("\n"))
            }
        }
    }

    /// Only ever delete paths of the form `<worktrees>/<branch>/<repo path>`.
    pub fn is_generated(&self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(self.worktrees_dir()) else { return false };
        rel.components().count() >= 2
    }

    /// The main repo of the checkout at `cwd`, verified to live under this root
    /// (and not inside the worktrees folder).
    pub fn main_repo_at(&self, cwd: &Path) -> Result<PathBuf> {
        let Some(main) = git::main_repo(cwd) else { bail!("Error: not inside a git repository.") };
        if !main.starts_with(&self.dir) || main.starts_with(self.worktrees_dir()) || main == self.dir {
            bail!("Error: {} is not a repo under {}", main.display(), self.dir.display());
        }
        Ok(main)
    }

    /// Remove now-empty directories from `dir` up to (excluding) the worktrees dir.
    pub fn cleanup_empty_dirs(&self, dir: &Path) {
        let mut cur = dir;
        while cur != self.worktrees_dir() && cur.starts_with(self.worktrees_dir()) {
            if !(cur.is_dir() && fs::remove_dir(cur).is_ok()) {
                return;
            }
            crate::out::say(format!("Removed empty {}", cur.display()));
            let Some(parent) = cur.parent() else { return };
            cur = parent;
        }
    }
}

/// Slashes in a branch name are flattened so it maps to one folder level.
pub fn flatten_branch(branch: &str) -> String {
    branch.replace('/', "-")
}

pub fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten() {
        assert_eq!(flatten_branch("feature/ABC-1/x"), "feature-ABC-1-x");
        assert_eq!(flatten_branch("plain"), "plain");
    }

    #[test]
    fn discovery_walks_up_and_scans_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        for wt in ["b1/repoA", "b1/ext/repoA", "b1/ext/other", "b2/repoA"] {
            fs::create_dir_all(root.join(".WORKTREES").join(wt)).unwrap();
            fs::write(root.join(".WORKTREES").join(wt).join(".git"), "gitdir: x").unwrap();
        }
        fs::create_dir_all(root.join(".WORKTREES/b3/ext/broken")).unwrap();
        for repo in ["repoA", "ext/repoA", "ext/other", "ext/repoA/sub", "notrepo/.hidden/r", "a/b/c/deep"] {
            fs::create_dir_all(root.join(repo).join(".git")).unwrap();
        }
        fs::write(root.join(CONFIG_FILE), Config::default().to_toml()).unwrap();

        assert_eq!(find_config(&root.join(".WORKTREES/b1/repoA")), Some(root.clone()));
        assert_eq!(find_config(tmp.path()), None);

        let r = Root::discover_from(&root.join("repoA")).unwrap();
        let names: Vec<String> = r.repos().iter().map(|p| r.repo_name(p)).collect();
        assert_eq!(names, vec!["a/b/c/deep", "ext/other", "ext/repoA", "repoA"], "nested at any depth; submodule/hidden skipped");
        let labels: Vec<String> = r.worktrees().iter().map(Worktree::label).collect();
        assert_eq!(labels, vec!["b1/ext/other", "b1/ext/repoA", "b1/repoA", "b2/repoA"]);
        assert!(r.is_generated(&root.join(".WORKTREES/b1/ext/repoA")));
        assert!(!r.is_generated(&root.join(".WORKTREES/b1")));
        assert!(!r.is_generated(&root.join("repoA")));
        assert_eq!(r.worktree_path("a/b", "ext/repoA"), root.join(".WORKTREES/a-b/ext/repoA"));

        // Shadowing: top-level wins, nested reachable by path or unique basename.
        assert_eq!(r.resolve_repo("repoA").unwrap(), root.join("repoA"));
        assert_eq!(r.resolve_repo("ext/repoA").unwrap(), root.join("ext/repoA"));
        assert_eq!(r.resolve_repo("other").unwrap(), root.join("ext/other"));
        assert_eq!(r.resolve_repo("OTH").unwrap(), root.join("ext/other"));
        assert!(r.resolve_repo("rep").unwrap_err().0.contains("Ambiguous"));
        assert!(r.resolve_repo("zzz").unwrap_err().0.contains("No repo"));

        r.cleanup_empty_dirs(&root.join(".WORKTREES/b3/ext/broken"));
        assert!(!root.join(".WORKTREES/b3").exists() && root.join(".WORKTREES").exists());
    }

    #[test]
    fn parent_from_gitdir_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        fs::create_dir(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: /home/x/Projects/alpha/.git/worktrees/alpha\n").unwrap();
        let w = Worktree { path: wt, branch_dir: "b".into(), repo: "alpha".into() };
        assert_eq!(w.parent(), Some(PathBuf::from("/home/x/Projects/alpha")));
    }
}
