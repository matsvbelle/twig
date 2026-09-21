//! twig prune: candidates, marks, filters, selection, -q.
use std::fs;
use std::path::Path;
use crate::util::fixture::{git, Fixture};

#[test]
fn prune_marks_filters_and_selection() {
    let f = Fixture::new();
    let a = f.repo("a");
    let b = f.repo("b");
    f.init();
    let push = |wt: &Path, branch: &str| git(wt, &["push", "-q", "-u", "origin", branch]);
    // a: gone (was pushed, deleted at origin), never-pushed, dirty+unpushed, still-alive.
    for br in ["gone", "local", "dirty", "alive"] {
        f.ok(&a, &[br]);
    }
    push(&f.wt("gone", "a"), "gone");
    git(&a, &["push", "-q", "origin", "--delete", "gone"]);
    push(&f.wt("alive", "a"), "alive");
    let dirty = f.wt("dirty", "a");
    push(&dirty, "dirty");
    fs::write(dirty.join("x"), "1").unwrap();
    git(&dirty, &["add", "."]);
    git(&dirty, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "unpushed"]);
    fs::write(dirty.join("y"), "2").unwrap();
    git(&a, &["push", "-q", "origin", "--delete", "dirty"]);
    // b: one never-pushed worktree, and a detached-HEAD one that must be skipped.
    f.ok(&b, &["b-local"]);
    f.ok(&b, &["detached"]);
    git(&f.wt("detached", "b"), &["checkout", "-q", "--detach"]);

    // Listing + marks, current repo only; abort with 'n'.
    let (ok, _, err) = f.interactive(&a, &["prune"], "\nn\n");
    assert!(ok, "{err}");
    assert!(err.contains("3 worktree(s)"), "{err}");
    assert!(err.contains("gone/a") && err.contains("local/a") && err.contains("[never pushed]") && err.contains("[gone]"), "{err}");
    assert!(err.contains("[dirty]") && err.contains("[1 unpushed]"), "{err}");
    assert!(!err.contains("alive/a") && !err.contains("b-local"), "{err}");
    assert!(err.contains("Aborted."));
    assert!(f.wt("gone", "a").exists());

    // -R hides never-pushed, -C hides dirty/unpushed.
    let (_, _, err) = f.interactive(&a, &["prune", "-R"], "\nn\n");
    assert!(err.contains("2 worktree(s)") && !err.contains("local/a"), "{err}");
    let (_, _, err) = f.interactive(&a, &["prune", "-C"], "\nn\n");
    assert!(err.contains("2 worktree(s)") && !err.contains("dirty/a"), "{err}");

    // -A spans repos, skips detached HEAD with a note.
    let (_, _, err) = f.interactive(&f.root, &["prune", "-A"], "\nn\n");
    assert!(err.contains("4 worktree(s)") && err.contains("b-local/b"), "{err}");
    assert!(err.contains("skipping detached/b"), "{err}");
    assert!(f.fail(&f.root, &["prune"]).contains("Use -A"));

    // Candidates are sorted (dirty, gone, local); excluding 1 and 3 prunes only 'gone'.
    let (ok, _, err) = f.interactive(&a, &["prune"], "1 3\ny\n");
    assert!(ok, "{err}");
    assert!(err.contains("Will prune 1 worktree(s)") && !f.wt("gone", "a").exists() && f.wt("dirty", "a").exists(), "{err}");
    let (_, _, err) = f.interactive(&a, &["prune"], "^9\n");
    assert!(err.contains("All candidates excluded"), "{err}");

    // From inside a pruned worktree: stdout is exactly the main repo path.
    let (ok, out, err) = f.interactive(&f.wt("local", "a"), &["prune", "-C"], "\ny\n");
    assert!(ok, "{err}");
    assert!(err.contains("[current]") && err.contains("you'll land in the main repo"), "{err}");
    assert_eq!(out.trim(), a.to_str().unwrap());
    assert!(!f.wt("local", "a").exists());
    assert!(f.clion_log().is_empty());

    // Nothing left for a. Without -q origin is never contacted; -q queries it.
    let (_, _, err) = f.interactive(&a, &["prune", "-C"], "");
    assert!(err.is_empty() || !err.contains("worktree(s)"), "{err}");
    let out = f.ok(&a, &["prune", "-C"]);
    assert!(out.contains("Nothing to prune for a") && out.contains("-q asks origin") && !out.contains("Querying"), "{out}");
    // A branch deleted at origin by someone else only shows up after a fetch, or with -q.
    f.ok(&a, &["elsewhere"]);
    push(&f.wt("elsewhere", "a"), "elsewhere");
    let origin = f.tmp.path().join("a.git");
    git(&origin, &["update-ref", "-d", "refs/heads/elsewhere"]);
    let out = f.ok(&a, &["prune", "-C"]);
    assert!(out.contains("Nothing to prune for a"), "{out}");
    let (_, _, err) = f.interactive(&a, &["prune", "-C", "-q"], "\nn\n");
    assert!(err.contains("Querying origin of a") && err.contains("elsewhere/a") && err.contains("[gone]"), "{err}");
    // Unreachable origin with -q skips b's worktrees with a warning.
    fs::remove_dir_all(f.tmp.path().join("b.git")).unwrap();
    let out = f.ok(&b, &["prune", "-q"]);
    assert!(out.contains("cannot query origin for b") && out.contains("Nothing to prune for b"), "{out}");
}
