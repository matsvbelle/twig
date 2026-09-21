//! twig init / status: activation, nesting, re-init options.
use std::fs;
use crate::util::fixture::Fixture;

#[test]
fn inactive_until_init() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    assert!(f.fail(&repo, &["list"]).contains("twig inactive"));
    assert!(String::from_utf8_lossy(&f.run(&repo, &["status"]).stdout).contains("twig inactive"));
    assert!(f.fail(&repo, &["init"]).contains("inside a git repository"));

    let out = f.ok(&f.root, &["init"]);
    assert!(out.contains("twig active for"), "{out}");
    assert!(f.root.join(".twig.toml").is_file());
    assert!(f.root.join(".WORKTREES").is_dir());
    let status = f.ok(&repo, &["status"]);
    assert!(status.contains("with 1 git repositories, 0 worktrees (.WORKTREES)"), "{status}");
    assert!(f.fail(&f.root, &["init"]).contains("already active"));

    // Re-init with options updates the tint; --no-tint drops the table.
    f.ok(&f.root, &["init", "--opacity", "12"]);
    assert!(fs::read_to_string(f.root.join(".twig.toml")).unwrap().contains("opacity = 12"));
    f.ok(&f.root, &["init", "--no-tint"]);
    let cfg = fs::read_to_string(f.root.join(".twig.toml")).unwrap();
    assert!(!cfg.contains("tint") && cfg.contains("ide = \"clion\""), "{cfg}");
    f.ok(&f.root, &["init", "--ide", "myide --wait"]);
    assert!(fs::read_to_string(f.root.join(".twig.toml")).unwrap().contains("ide = \"myide --wait\""));
    assert!(f.ok(&repo, &["status"]).contains("ide: myide --wait"));
}

#[test]
fn init_custom_name_and_nesting_refused() {
    let f = Fixture::new();
    f.ok(&f.root, &["init", "trees"]);
    assert!(f.root.join("trees").is_dir());
    let inner = f.root.join("sub");
    fs::create_dir_all(&inner).unwrap();
    assert!(f.fail(&inner, &["init"]).contains("nested"));
    let outer = f.tmp.path();
    assert!(f.fail(outer, &["init"]).contains("nested"));
    let warn = String::from_utf8_lossy(&f.cmd(&f.root, &["status"]).env_remove("TWIG_SHELL").output().unwrap().stderr).to_string();
    assert!(warn.contains("shell integration not active"), "{warn}");
}
