//! Per-worktree state shared by `list -l` and `prune`. Everything is read from
//! the local clone (what the last fetch left behind); only `prune -q` asks origin.
use crate::git;
use crate::out;
use crate::root::Worktree;
use std::collections::HashSet;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Status {
    pub dirty: bool,
    pub unpushed: u32,
    /// No upstream configured: the branch was never pushed (or tracking was never set up).
    pub never_pushed: bool,
    /// Upstream configured but its remote-tracking ref is gone (`git branch -vv` shows `[gone]`).
    pub gone: bool,
}

impl Status {
    /// `remote_heads`: branch names at origin, when the caller queried it.
    pub fn probe(wt: &Worktree, branch: &str, remote_heads: Option<&HashSet<String>>) -> Status {
        let track = git::run(&wt.path, &["for-each-ref", "--format=%(upstream:track)", &format!("refs/heads/{branch}")]).unwrap_or_default();
        let has_upstream = git::run(&wt.path, &["config", &format!("branch.{branch}.remote")]).is_ok_and(|r| !r.is_empty());
        let never_pushed = !has_upstream;
        let gone = has_upstream && (track == "[gone]" || remote_heads.is_some_and(|h| !h.contains(branch)));
        let dirty = git::run(&wt.path, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty());
        let unpushed = git::run(&wt.path, &["rev-list", "--count", "HEAD", "--not", "--remotes"]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        Status { dirty, unpushed, never_pushed, gone }
    }

    /// Bracketed marks, e.g. ` [dirty] [2 unpushed] [gone]`; empty when clean and tracked.
    pub fn marks(&self) -> String {
        let mut s = String::new();
        if self.dirty {
            s.push_str(&format!(" {}", out::yellow("[dirty]")));
        }
        if self.unpushed > 0 {
            s.push_str(&format!(" {}", out::yellow(&format!("[{} unpushed]", self.unpushed))));
        }
        if self.never_pushed {
            s.push_str(&format!(" {}", out::red("[never pushed]")));
        }
        if self.gone {
            s.push_str(&format!(" {}", out::red("[gone]")));
        }
        s
    }
}
