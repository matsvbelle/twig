//! twig open / exit / -o and --help on every subcommand.
use crate::util::fixture::Fixture;

#[test]
fn open_current_checkout_and_help_everywhere() {
    let f = Fixture::new();
    let a = f.repo("a");
    f.init();
    f.ok(&a, &["feat"]);
    let wt = f.wt("feat", "a");
    let out = f.run(&wt, &["-o"]);
    assert!(out.status.success() && out.stdout.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(f.wait_clion(wt.to_str().unwrap()));
    let out = f.run(&a, &["-o"]);
    assert!(out.status.success() && out.stdout.is_empty());
    assert!(f.wait_clion(&format!("{}\n", a.display())));
    assert!(f.fail(&f.root, &["-o"]).contains("Not inside a git repo"));

    for (cmd, flag) in [("list", "--long"), ("open", "--open"), ("exit", "--open"), ("remove", "--list"), ("prune", "--query"), ("init", "--ide"), ("status", "--open"), ("update", "--check")] {
        let help = f.ok(&f.root, &[cmd, "--help"]);
        assert!(help.contains("Usage: twig ") && help.contains(flag), "{cmd}: {help}");
        assert_eq!(f.ok(&f.root, &["-o", cmd, "--help"]), help, "-o before {cmd}");
    }
    let help = f.ok(&f.root, &["open", "--help"]);
    assert!(help.contains("[NAME]  Repo:"), "{help}");
    let help = f.ok(&f.root, &["list", "--help"]);
    assert!(help.contains("-r, --root-repos") && help.contains("-i, --interactive-switch"), "{help}");
}
