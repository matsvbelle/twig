//! Per-worktree state shared by `list -l` and `prune`. Everything is read from
//! the local clone (what the last fetch left behind); only `prune -q` asks origin.
use crate::git;
use crate::out;
use std::collections::HashSet;
use std::path::Path;

/// One `probe` call: checkout, branch, remote heads if queried.
pub type Job<'a> = (&'a Path, &'a str, Option<&'a HashSet<String>>);

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
    /// `checkout`: the worktree or main repo on `branch`. `remote_heads`: branch names at origin, when the caller queried it.
    pub fn probe(checkout: &Path, branch: &str, remote_heads: Option<&HashSet<String>>) -> Status {
        let track = git::run(checkout, &["for-each-ref", "--format=%(upstream:track)", &format!("refs/heads/{branch}")]).unwrap_or_default();
        let has_upstream = git::run(checkout, &["config", &format!("branch.{branch}.remote")]).is_ok_and(|r| !r.is_empty());
        let never_pushed = !has_upstream;
        let gone = has_upstream && (track == "[gone]" || remote_heads.is_some_and(|h| !h.contains(branch)));
        // Not recursing into submodules saves a git process per submodule; a moved submodule pointer still counts.
        let dirty = git::run(checkout, &["status", "--porcelain", "--ignore-submodules=dirty"]).is_ok_and(|s| !s.is_empty());
        let unpushed = git::run(checkout, &["rev-list", "--count", "HEAD", "--not", "--remotes"]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        Status { dirty, unpushed, never_pushed, gone }
    }

    /// `probe` for many checkouts at once, spread over threads; results in input order.
    pub fn probe_all(jobs: &[Job]) -> Vec<Status> {
        if jobs.is_empty() {
            return Vec::new();
        }
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(jobs.len());
        std::thread::scope(|s| {
            let handles: Vec<_> = jobs.chunks(jobs.len().div_ceil(threads)).map(|chunk| s.spawn(move || chunk.iter().map(|(p, b, h)| Status::probe(p, b, *h)).collect::<Vec<_>>())).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        })
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
