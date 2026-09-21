//! twig shell / __complete.
use crate::util::fixture::Fixture;

#[test]
fn completion_and_shell_snippet() {
    let f = Fixture::new();
    let repo = f.repo("alpha");
    assert_eq!(f.ok(&repo, &["__complete", "repos"]).trim(), "");
    f.init();
    f.ok(&repo, &["b1"]);
    assert_eq!(f.ok(&repo, &["__complete", "repos"]).trim(), "alpha");
    assert_eq!(f.ok(&repo, &["__complete", "worktrees"]).trim(), "b1");
    let branches = f.ok(&repo, &["__complete", "branches"]);
    assert!(branches.contains("main") && branches.contains("b1") && !branches.contains("HEAD"));
    let snippet = f.ok(&f.root, &["shell", "zsh"]);
    assert!(snippet.contains("compdef _twig twig") && snippet.contains("__twig_list_switches"), "{snippet}");
    assert!(f.fail(&f.root, &["shell", "fish"]).contains("unsupported"));
}
