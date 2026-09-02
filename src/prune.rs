//! `twig prune`: interactively remove worktrees whose branch is gone from origin.
//! By default "gone" is judged from the last fetch; `-q` asks origin.
use crate::cli::PruneArgs;
use crate::error::Result;
use crate::remove::Remover;
use crate::root::{name_of, Root, Worktree};
use crate::status::{Job, Status};
use crate::{git, list, out};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

struct Candidate {
    wt: Worktree,
    branch: String,
    marks: String,
}

pub fn run(args: PruneArgs, open: bool) -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    let repo_filter = if args.all { None } else { Some(list::current_repo(&root).map_err(|_| "Not inside a git repo. Use -A to prune across all repos.")?) };
    let all = root.worktrees();
    if all.is_empty() {
        out::say(format!("No worktrees under {}", root.worktrees_dir().display()));
        return Ok(());
    }
    let remover = Remover::new(root.clone(), open);

    let mut remote_heads: HashMap<PathBuf, Option<HashSet<String>>> = HashMap::new();
    let mut pending: Vec<(Worktree, String, PathBuf)> = Vec::new();
    for wt in all.into_iter().filter(|w| repo_filter.as_deref().is_none_or(|r| w.repo == r)) {
        let label = wt.label();
        let Some(branch) = wt.branch() else {
            out::warn(format!("note: skipping {label} (detached HEAD or broken worktree)"));
            continue;
        };
        let Some(parent) = wt.parent().filter(|p| p.join(".git").exists()) else {
            out::warn(format!("note: skipping {label} (cannot resolve parent repo)"));
            continue;
        };
        if args.query && remote_heads.entry(parent.clone()).or_insert_with(|| query_origin(&parent)).is_none() {
            continue;
        }
        pending.push((wt, branch, parent));
    }
    let jobs: Vec<Job> = pending.iter().map(|(wt, branch, parent)| (wt.path.as_path(), branch.as_str(), remote_heads.get(parent).and_then(Option::as_ref))).collect();
    let statuses = Status::probe_all(&jobs);

    let mut candidates = Vec::new();
    for ((wt, branch, _), status) in pending.into_iter().zip(statuses) {
        if !(status.gone || status.never_pushed) || (args.skip_local && status.never_pushed) {
            continue;
        }
        if args.skip_dirty && (status.dirty || status.unpushed > 0) {
            continue;
        }
        let mut marks = status.marks();
        if remover.current_top.as_deref() == Some(wt.path.as_path()) {
            marks = format!(" {}{marks}", out::green("[current]"));
        }
        candidates.push(Candidate { wt, branch, marks });
    }

    if candidates.is_empty() {
        let hint = if args.query { "" } else { " (as of the last fetch; -q asks origin)" };
        match &repo_filter {
            None => out::say(format!("Nothing to prune: every worktree's branch still exists at origin{hint}.")),
            Some(r) => out::say(format!("Nothing to prune for {r}: every worktree's branch still exists at origin{hint}.")),
        }
        return Ok(());
    }

    out::say("");
    out::say(out::bold(&format!(":: {} worktree(s) whose branch is gone from origin:", candidates.len())));
    for (i, c) in candidates.iter().enumerate() {
        out::say(format!(" {:3}  {:<42} {}{}", i + 1, c.wt.label(), out::dim(&format!("[{}]", c.branch)), c.marks));
    }
    let reply = prompt("==> Worktrees to exclude (eg: 1 2 3, 1-3, ^4; Enter = none): ")?;
    let selected = parse_exclusions(&reply, candidates.len());
    if selected.is_empty() {
        out::say("All candidates excluded; nothing to do.");
        return Ok(());
    }

    out::say("");
    out::say(out::bold(&format!("Will prune {} worktree(s):", selected.len())));
    for &i in &selected {
        let c = &candidates[i];
        let note = if c.marks.contains("[current]") {
            out::yellow(if open { " (current worktree — the IDE switches to the main repo)" } else { " (current worktree — you'll land in the main repo)" })
        } else {
            String::new()
        };
        out::say(format!("  {}{note}", c.wt.label()));
    }
    let ans = prompt("==> Proceed? [y/N] ")?;
    if !ans.trim_start().to_lowercase().starts_with('y') {
        out::say("Aborted.");
        return Ok(());
    }
    out::say("");

    let targets: Vec<PathBuf> = selected.iter().map(|&i| candidates[i].wt.path.clone()).collect();
    remover.remove_all(&targets)?;
    out::say("Done.");
    Ok(())
}

/// Branch names at origin, or None (with a warning) if origin is unreachable.
fn query_origin(parent: &Path) -> Option<HashSet<String>> {
    out::warn(format!("Querying origin of {}...", name_of(parent)));
    match git::run(parent, &["ls-remote", "--heads", "origin"]) {
        Ok(out) => Some(out.lines().filter_map(|l| l.rsplit("refs/heads/").next().map(str::to_string)).collect()),
        Err(_) => {
            out::warn(format!("WARNING: cannot query origin for {}; skipping its worktrees.", name_of(parent)));
            None
        }
    }
}

fn prompt(msg: &str) -> Result<String> {
    eprint!("{msg}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}

/// Yay-style selection: plain tokens exclude those numbers (`1 2`, `1-3`);
/// `^n` inverts (exclude everything EXCEPT n). Returns 0-based selected indices.
pub fn parse_exclusions(line: &str, count: usize) -> Vec<usize> {
    let mut excluded = HashSet::new();
    let mut kept = HashSet::new();
    let mut invert = false;
    for tok in line.split_whitespace() {
        let (neg, tok) = match tok.strip_prefix('^') {
            Some(t) => (true, t),
            None => (false, tok),
        };
        let range = match tok.split_once('-') {
            Some((lo, hi)) => lo.parse::<usize>().ok().zip(hi.parse::<usize>().ok()),
            None => tok.parse::<usize>().ok().map(|n| (n, n)),
        };
        let Some((lo, hi)) = range else {
            out::warn(format!("  (ignoring '{tok}')"));
            continue;
        };
        invert |= neg;
        for n in lo..=hi {
            if (1..=count).contains(&n) {
                if neg { kept.insert(n) } else { excluded.insert(n) };
            }
        }
    }
    (1..=count)
        .filter(|n| !excluded.contains(n) && (!invert || kept.contains(n)))
        .map(|n| n - 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_syntax() {
        assert_eq!(parse_exclusions("", 4), vec![0, 1, 2, 3]);
        assert_eq!(parse_exclusions("1 3", 4), vec![1, 3]);
        assert_eq!(parse_exclusions("1-3", 4), vec![3]);
        assert_eq!(parse_exclusions("^4", 4), vec![3]);
        assert_eq!(parse_exclusions("^1-2 junk 9", 4), vec![0, 1]);
        assert_eq!(parse_exclusions("1 2 3 4", 4), Vec::<usize>::new());
    }
}
