//! twig <branch>: worktree creation, base refs, .idea seeding, hook, submodules.
use std::fs;
use std::process::Command;
use crate::util::fixture::{git, Fixture};

#[test]
fn add_list_exit_remove_flow() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    fs::create_dir_all(repo.join(".idea/codeStyles")).unwrap();
    fs::write(repo.join(".idea/misc.xml"), "<project/>").unwrap();
    fs::write(repo.join(".gitlab_token"), "tok").unwrap();
    f.init();

    // New branch from HEAD; stdout is exactly the path for the shell wrapper to cd into.
    let out = f.run(&repo, &["feature/x"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let wt = f.root.join(".WORKTREES/feature-x/alpha");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), wt.to_str().unwrap());
    assert_eq!(git(&wt, &["branch", "--show-current"]), "feature/x");
    assert!(fs::symlink_metadata(wt.join(".idea/codeStyles")).unwrap().file_type().is_symlink());
    assert!(wt.join(".idea/misc.xml").is_file());
    assert!(wt.join(".idea/worktree-bg.png").is_file());
    let ws = fs::read_to_string(wt.join(".idea/workspace.xml")).unwrap();
    assert!(ws.contains("idea.background.editor") && ws.contains("ProjectColorInfo"), "{ws}");
    assert!(fs::read_to_string(repo.join(".idea/workspace.xml")).unwrap().contains("ProjectColorInfo"));
    assert!(fs::symlink_metadata(wt.join(".gitlab_token")).unwrap().file_type().is_symlink());
    assert!(f.clion_log().is_empty());

    // Branch pin: switching inside the worktree is refused, commits are fine.
    let hook = repo.join(".git/hooks/reference-transaction");
    assert!(hook.is_file());
    git(&repo, &["switch", "-q", "-c", "other"]);
    git(&repo, &["switch", "-q", "main"]);
    let sw = Command::new("git").current_dir(&wt).args(["switch", "other"]).output().unwrap();
    assert!(!sw.status.success());
    assert!(String::from_utf8_lossy(&sw.stderr).contains("pinned"), "{}", String::from_utf8_lossy(&sw.stderr));
    assert_eq!(git(&wt, &["branch", "--show-current"]), "feature/x");

    // Existing worktree: -o opens it in the IDE instead of printing the path.
    let again = f.run(&repo, &["-o", "feature/x"]);
    assert!(again.status.success() && String::from_utf8_lossy(&again.stdout).is_empty());
    assert!(String::from_utf8_lossy(&again.stderr).contains("already exists"));
    assert!(f.wait_clion(wt.to_str().unwrap()));

    // Existing local branch and origin-only branch.
    f.ok(&repo, &["other"]);
    git(&repo, &["push", "-q", "origin", "main:remote-only"]);
    f.ok(&repo, &["remote-only"]);
    let ro = f.root.join(".WORKTREES/remote-only/alpha");
    assert_eq!(git(&ro, &["rev-parse", "--abbrev-ref", "@{u}"]), "origin/remote-only");

    // List (from inside a worktree resolves the main repo).
    let list = f.ok(&wt, &["list"]);
    assert!(list.contains("feature-x/") && list.contains("[feature/x]") && list.contains("remote-only"), "{list}");
    assert!(f.ok(&f.root, &["list", "-A"]).contains("(all repos)"));
    assert!(f.fail(&f.root, &["list"]).contains("Use -A"));

    // Exit: cd to the main repo by default, -o opens it.
    let ex = f.run(&wt, &["exit"]);
    assert_eq!(String::from_utf8_lossy(&ex.stdout).trim(), repo.to_str().unwrap());
    assert!(f.ok(&wt, &["exit", "-o"]).contains("Main repo:"));
    assert!(f.wait_clion(repo.to_str().unwrap()));

    // Open: exact, substring, listing.
    let o = f.run(&f.root, &["open", "alp"]);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), repo.to_str().unwrap());
    assert!(f.ok(&f.root, &["open"]).contains("alpha"));
    assert!(f.fail(&f.root, &["open", "nope"]).contains("No repo"));

    // Remove by <branch> <repo>, by branch (all repos), refusing non-generated paths.
    f.ok(&f.root, &["remove", "remote-only", "alpha"]);
    assert!(!ro.exists() && !ro.parent().unwrap().exists());
    assert!(!git(&repo, &["worktree", "list"]).contains("remote-only"));
    assert!(f.fail(&f.root, &["remove", repo.to_str().unwrap()]).contains("Refusing"));
    f.ok(&f.root, &["remove", "other"]);
    assert!(!f.root.join(".WORKTREES/other").exists());
    assert!(f.ok(&f.root, &["remove", "-l"]).contains("feature-x/alpha"));
    // Branch survives removal.
    assert!(git(&repo, &["branch", "--list", "other"]).contains("other"));
}

#[test]
fn help_escape_hatch_and_base_ref() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    f.init();
    let help = f.run(&repo, &[]);
    assert!(help.status.success() && String::from_utf8_lossy(&help.stdout).contains("Usage"));

    // A branch named like a subcommand needs `--`.
    f.ok(&repo, &["--", "list"]);
    assert_eq!(git(&f.wt("list", "alpha"), &["branch", "--show-current"]), "list");

    // Explicit base ref: new branch starts at that commit, not HEAD.
    let base = git(&repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join("more"), "x").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "second"]);
    f.ok(&repo, &["from-base", &base]);
    assert_eq!(git(&f.wt("from-base", "alpha"), &["rev-parse", "HEAD"]), base);

    // Works from inside a worktree: resolves the main repo through the common dir.
    f.ok(&f.wt("from-base", "alpha"), &["nested-add"]);
    assert!(f.wt("nested-add", "alpha").is_dir());
    assert!(git(&repo, &["worktree", "list"]).contains("nested-add"));

    // A repo outside the twigged dir is refused even though twig is active.
    let stray = f.root.join(".WORKTREES/stray");
    fs::create_dir_all(&stray).unwrap();
    git(&stray, &["init", "-q"]);
    assert!(f.fail(&stray, &["b"]).contains("not a repo under"));
}

#[test]
fn idea_optional_and_tint_off() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    f.init();
    // No .idea in main → nothing IDE-related is created.
    f.ok(&repo, &["plain"]);
    assert!(!f.wt("plain", "alpha").join(".idea").exists());

    fs::create_dir_all(repo.join(".idea")).unwrap();
    f.ok(&f.root, &["init", "--no-tint"]);
    f.ok(&repo, &["notint"]);
    let idea = f.wt("notint", "alpha").join(".idea");
    assert!(!idea.join("worktree-bg.png").exists());
    let ws = fs::read_to_string(idea.join("workspace.xml")).unwrap();
    assert!(ws.contains("ProjectColorInfo") && !ws.contains("idea.background"), "{ws}");
    assert!(f.ok(&repo, &["status"]).contains("tint: off"));

    // -o uses the configured IDE command (program + args, path appended).
    let myide = f.bin.join("myide");
    fs::write(&myide, format!("#!/bin/sh\necho \"MYIDE $@\" >> '{}'\n", f.tmp.path().join("clion.log").display())).unwrap();
    fs::set_permissions(&myide, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    f.ok(&f.root, &["init", "--ide", "myide --wait"]);
    f.ok(&repo, &["open", "-o", "alpha"]);
    assert!(f.wait_clion(&format!("MYIDE --wait {}", repo.display())));
    f.ok(&repo, &["-o", "open", "alpha"]);
    assert!(f.wait_clion(&format!("MYIDE --wait {}", repo.display())));
    assert!(f.ok(&repo, &["-o", "notint"]).contains("Opening in myide"));
}

#[test]
fn submodules_seeded_from_main_repo() {
    let f = Fixture::new();
    let lib = f.repo("lib");
    let app = f.repo("app");
    let lib_origin = f.tmp.path().join("lib.git");
    git(&app, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", lib_origin.to_str().unwrap(), "libs/lib"]);
    git(&app, &["commit", "-qm", "submodule"]);
    f.init();
    // Make the origin unreachable: seeding must work from local objects alone.
    fs::rename(&lib_origin, f.tmp.path().join("lib.git.moved")).unwrap();
    let out = f.ok(&app, &["sm"]);
    assert!(out.contains("Seeding submodules"), "{out}");
    let wt = f.wt("sm", "app");
    assert!(wt.join("libs/lib/README").is_file());
    let status = git(&wt, &["submodule", "status"]);
    assert!(!status.starts_with('-') && !status.starts_with('+'), "submodule not checked out: {status}");

    // -l: a modified file inside a submodule is not "dirty"; a moved submodule pointer is.
    let sub = wt.join("libs/lib");
    fs::write(sub.join("README"), "edited").unwrap();
    assert!(!f.ok(&wt, &["list", "-l"]).contains("[dirty]"));
    git(&sub, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qam", "bump"]);
    assert!(f.ok(&wt, &["list", "-l"]).contains("[sm] ← here [dirty]"));
    drop(lib);
}

#[test]
fn branch_pin_hook_coexistence() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    f.init();
    let hook = repo.join(".git/hooks/reference-transaction");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();

    // An unrelated hook is left alone.
    fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
    let out = f.ok(&repo, &["a"]);
    assert!(out.contains("unrelated reference-transaction hook"), "{out}");
    assert_eq!(fs::read_to_string(&hook).unwrap(), "#!/bin/sh\nexit 0\n");

    // An outdated copy of our own hook is overwritten with the embedded version.
    fs::write(&hook, "#!/bin/sh\n# twig-branch-pin\nexit 0\n").unwrap();
    f.ok(&repo, &["b"]);
    assert!(fs::read_to_string(&hook).unwrap().contains("Refusing to switch"));

    // Pin survives a bare `git commit` in the worktree (only branch switches are blocked).
    let wt = f.wt("b", "alpha");
    fs::write(wt.join("f"), "1").unwrap();
    git(&wt, &["add", "."]);
    git(&wt, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "ok"]);
    // Override switch once.
    let sw = Command::new("git").current_dir(&wt).env("WORKTREE_ALLOW_SWITCH", "1").args(["switch", "-q", "-c", "escaped"]).status().unwrap();
    assert!(sw.success());
}
