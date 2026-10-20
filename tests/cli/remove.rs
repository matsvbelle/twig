//! twig remove: by branch/repo/path, current worktree, detached worker.
use std::fs;
use crate::util::fixture::{git, Fixture};

#[test]
fn remove_edge_cases() {
    let f = Fixture::new();
    let a = f.repo("a");
    let b = f.repo("b");
    f.init();
    assert!(f.fail(&f.root, &["remove"]).contains("Not inside a git worktree"));
    assert!(f.fail(&a, &["remove"]).contains("not a generated worktree"));
    assert!(f.fail(&a, &["remove", "nope"]).contains("No worktree path or branch"));

    // Same branch in two repos: removing one keeps the shared branch dir.
    f.ok(&a, &["shared"]);
    f.ok(&b, &["shared"]);
    f.ok(&f.root, &["remove", "shared", "a"]);
    assert!(!f.wt("shared", "a").exists() && f.wt("shared", "b").exists());
    assert!(f.fail(&f.root, &["remove", "shared", "a"]).contains("No worktree of a"));

    // Read-only build output is still deleted (chmod u+w before rm).
    let ro = f.wt("shared", "b").join("build");
    fs::create_dir_all(&ro).unwrap();
    fs::write(ro.join("obj"), "x").unwrap();
    fs::set_permissions(&ro, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    f.ok(&f.root, &["remove", "shared"]);
    assert!(!f.root.join(".WORKTREES/shared").exists());
    assert!(!git(&b, &["worktree", "list"]).contains("shared"));
    assert!(f.ok(&f.root, &["remove", "-l"]).contains("No worktrees"));

    // Removing a worktree from a sibling repo's cwd never touches CLion.
    f.ok(&a, &["other"]);
    f.ok(&b, &["remove", f.wt("other", "a").to_str().unwrap()]);
    assert!(f.clion_log().is_empty());
}

#[test]
fn remove_current_worktree_is_detached_and_prune_flow() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    f.init();
    f.ok(&repo, &["gone"]);
    f.ok(&repo, &["kept"]);
    let gone = f.root.join(".WORKTREES/gone/alpha");
    git(&gone, &["push", "-q", "-u", "origin", "gone"]);
    git(&f.root.join(".WORKTREES/kept/alpha"), &["push", "-q", "-u", "origin", "kept"]);
    git(&repo, &["push", "-q", "origin", "--delete", "gone"]);

    // Prune, non-interactive via stdin: exclude nothing, confirm.
    let (ok, _, err) = f.interactive(&repo, &["prune"], "\ny\n");
    assert!(ok, "{err}");
    assert!(err.contains("1 worktree(s) whose branch is gone") && err.contains("gone/alpha"), "{err}");
    assert!(!err.contains("kept/alpha"));
    assert!(!gone.exists());
    assert!(f.root.join(".WORKTREES/kept/alpha").exists());

    // Removing the worktree we're in: by default synchronous, landing in the main repo.
    f.ok(&repo, &["sync"]);
    let sync = f.wt("sync", "alpha");
    let out = f.run(&sync, &["remove"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), repo.to_str().unwrap());
    assert!(!sync.exists() && f.clion_log().is_empty());

    // With -o: detached worker, IDE switched to main.
    let kept = f.root.join(".WORKTREES/kept/alpha");
    let out = f.ok(&kept, &["remove", "-o"]);
    assert!(out.contains("in the background"), "{out}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let branch_dir = kept.parent().unwrap();
    while branch_dir.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(!branch_dir.exists(), "detached removal did not finish");
    assert!(f.wait_clion(repo.to_str().unwrap()));
    assert!(!git(&repo, &["worktree", "list"]).contains("kept"));
}

fn commit(dir: &std::path::Path, file: &str) {
    fs::write(dir.join(file), file).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-qm", file]);
}

#[test]
fn remove_deletes_local_branch_only_when_safe() {
    let f = Fixture::new();
    let a = f.repo("a");
    f.init();
    let has_branch = |b: &str| !git(&a, &["branch", "--list", b]).is_empty();

    // No commits of its own, or all pushed: the local branch goes, the remote one stays.
    f.ok(&a, &["fresh"]);
    f.ok(&f.root, &["remove", "fresh"]);
    assert!(!has_branch("fresh"));
    f.ok(&a, &["pushed"]);
    commit(&f.wt("pushed", "a"), "p");
    git(&f.wt("pushed", "a"), &["push", "-q", "-u", "origin", "pushed"]);
    let out = f.ok(&f.root, &["remove", "pushed"]);
    assert!(out.contains("Deleted local branch 'pushed'") && !has_branch("pushed"), "{out}");
    assert!(!git(&a, &["ls-remote", "origin", "pushed"]).is_empty());

    // Unpushed commits: kept, unless -f.
    f.ok(&a, &["local"]);
    commit(&f.wt("local", "a"), "l");
    let out = f.ok(&f.root, &["remove", "local"]);
    assert!(out.contains("Kept branch 'local'") && has_branch("local"), "{out}");
    f.ok(&a, &["local"]);
    f.ok(&f.root, &["remove", "-f", "local"]);
    assert!(!has_branch("local"));
}

#[test]
fn prune_and_list_pass_force_on() {
    let f = Fixture::new();
    let a = f.repo("a");
    f.init();
    let has_branch = |b: &str| !git(&a, &["branch", "--list", b]).is_empty();
    let gone_with_commit = |b: &str| {
        f.ok(&a, &[b]);
        commit(&f.wt(b, "a"), b);
        git(&f.wt(b, "a"), &["push", "-q", "-u", "origin", b]);
        git(&a, &["push", "-q", "origin", "--delete", b]);
    };

    gone_with_commit("g1");
    let (ok, _, err) = f.interactive(&a, &["prune"], "\ny\n");
    assert!(ok && err.contains("Kept branch 'g1'") && has_branch("g1"), "{err}");
    gone_with_commit("g2");
    let (ok, _, err) = f.interactive(&a, &["prune", "-f"], "\ny\n");
    assert!(ok && !f.wt("g2", "a").exists() && !has_branch("g2"), "{err}");

    f.ok(&a, &["menu"]);
    commit(&f.wt("menu", "a"), "m");
    let (ok, _, err) = f.interactive(&a, &["list", "-i", "-f"], "ry");
    assert!(ok && !f.wt("menu", "a").exists() && !has_branch("menu"), "{err}");
}
