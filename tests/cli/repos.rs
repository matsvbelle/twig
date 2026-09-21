//! Nested repos: mirrored layout, name shadowing, resolution order.
use std::fs;
use crate::util::fixture::{git, Fixture};

#[test]
fn nested_repos_mirror_layout_and_shadowing() {
    let f = Fixture::new();
    let top = f.repo("shadowed");
    let nested = f.repo("external/shadowed");
    let only = f.repo("external/only");
    // A submodule-like repo inside a repo is not a separate repo.
    fs::create_dir_all(top.join("vendor/inner/.git")).unwrap();
    f.init();
    assert!(f.ok(&f.root, &["status"]).contains("with 3 git repositories"));
    let repos = f.ok(&f.root, &["open"]);
    assert!(repos.contains("external/only") && repos.contains("external/shadowed") && !repos.contains("vendor"), "{repos}");

    // Worktrees mirror the relative path; both `shadowed` repos coexist on one branch.
    f.ok(&nested, &["b"]);
    f.ok(&top, &["b"]);
    let nested_wt = f.root.join(".WORKTREES/b/external/shadowed");
    assert!(nested_wt.is_dir() && f.wt("b", "shadowed").is_dir());
    assert_eq!(git(&nested_wt, &["branch", "--show-current"]), "b");
    // From inside the nested worktree, list resolves the nested main repo.
    let l = f.ok(&nested_wt, &["list"]);
    assert!(l.contains("(repo: external/shadowed)") && l.contains("external/shadowed"), "{l}");
    let la = f.ok(&f.root, &["list", "-A"]);
    assert!(la.contains("external/shadowed") && la.contains("shadowed "), "{la}");

    // open: top-level wins; nested via path or unique basename.
    let path = |args: &[&str]| String::from_utf8_lossy(&f.run(&f.root, args).stdout).trim().to_string();
    assert_eq!(path(&["open", "shadowed"]), top.to_str().unwrap());
    assert_eq!(path(&["open", "external/shadowed"]), nested.to_str().unwrap());
    assert_eq!(path(&["open", "only"]), only.to_str().unwrap());
    assert_eq!(f.ok(&f.root, &["__complete", "repos"]).trim(), "external/only\nexternal/shadowed\nshadowed");

    // remove <branch> <repo> follows the same rule and cleans empty parents.
    f.ok(&f.root, &["remove", "b", "shadowed"]);
    assert!(!f.wt("b", "shadowed").exists() && nested_wt.exists());
    assert!(f.fail(&f.root, &["remove", "b", "only"]).contains("No worktree of external/only"));
    f.ok(&f.root, &["remove", "b", "external/shadowed"]);
    assert!(!f.root.join(".WORKTREES/b").exists());
    assert!(!git(&nested, &["worktree", "list"]).contains("/b/"));
    // Whole-branch removal spans nested worktrees too.
    f.ok(&nested, &["c"]);
    f.ok(&only, &["c"]);
    f.ok(&f.root, &["remove", "c"]);
    assert!(!f.root.join(".WORKTREES/c").exists());
}
