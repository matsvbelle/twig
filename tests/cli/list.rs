//! twig list: marks, -l status, -r tree, -i menu, repo scoping.
use std::fs;
use crate::util::fixture::{git, Fixture};

#[test]
fn list_marks_current_and_long_status() {
    let f = Fixture::new();
    let a = f.repo("a");
    f.init();
    f.ok(&a, &["clean"]);
    f.ok(&a, &["work"]);
    git(&f.wt("clean", "a"), &["push", "-q", "-u", "origin", "clean"]);
    let work = f.wt("work", "a");
    fs::write(work.join("x"), "1").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "unpushed"]);
    fs::write(work.join("y"), "2").unwrap();

    let plain = f.ok(&work, &["list"]);
    assert!(plain.contains("[work] ← here") && !plain.contains("[clean] ← here") && !plain.contains("dirty"), "{plain}");
    assert!(!f.ok(&a, &["list"]).contains("← here"));

    let long = f.ok(&work, &["list", "-l"]);
    assert!(long.contains("[work] ← here [dirty] [1 unpushed] [never pushed]"), "{long}");
    let clean_line = long.lines().find(|l| l.contains("[clean]")).unwrap();
    assert!(!clean_line.contains("dirty") && !clean_line.contains("pushed") && !clean_line.contains("gone"), "{long}");
    assert!(!long.contains("Querying"), "{long}");

    // Colour only on a terminal, so none when piped.
    assert!(!plain.contains("\x1b["));
}

#[test]
fn list_root_repos_and_interactive_switch() {
    let f = Fixture::new();
    let a = f.repo("a");
    let b = f.repo("b");
    f.init();
    f.ok(&a, &["one"]);
    f.ok(&a, &["two"]);
    f.ok(&b, &["x"]);
    let two = f.wt("two", "a");
    git(&two, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "ahead"]);

    // -r: the main repos, with the worktrees folder nested below; -l adds their branch.
    let r = f.ok(&a, &["list", "-r"]);
    let tree = format!("Projects  (repo: a)\n\u{251c}\u{2500}\u{2500} {:<26} [main] \u{2190} here\n\u{2514}\u{2500}\u{2500} .WORKTREES/\n    \u{251c}\u{2500}\u{2500} one/\n    \u{2502}   \u{2514}\u{2500}\u{2500} {:<18} [one]\n", "a", "a");
    assert!(r.contains(&tree) && !r.contains("x/"), "{r}");
    let rl = f.ok(&f.root, &["list", "-A", "-r", "-l"]);
    let roots = format!("\u{251c}\u{2500}\u{2500} {:<26} [main]\n\u{251c}\u{2500}\u{2500} {:<26} [main]\n\u{2514}\u{2500}\u{2500} .WORKTREES/\n", "a", "b");
    assert!(rl.contains(&roots) && rl.contains(&format!("        \u{2514}\u{2500}\u{2500} {:<18} [x] [never pushed]\n", "b")) && !rl.contains("here"), "{rl}");

    // -i: arrows move, Enter/Space print the highlighted path (and nothing else) on stdout.
    let (ok, out, all) = f.interactive(&a, &["list", "-i"], "\x1b[B\r");
    assert!(ok && out.trim() == two.to_str().unwrap(), "{all}");
    assert!(all.contains("> ") && all.contains("\u{2192} two/a"), "{all}");
    let (ok, out, _) = f.interactive(&two, &["list", "-i"], "\r");
    assert!(ok && out.trim() == two.to_str().unwrap(), "starts on the checkout we're in");
    let (ok, out, _) = f.interactive(&a, &["list", "-i", "-r"], " ");
    assert!(ok && out.trim() == a.to_str().unwrap(), "main repo rows are selectable");
    for input in ["q", "\x1b", "", "\x03", "nabc\x1bq"] {
        let (ok, out, all) = f.interactive(&a, &["list", "-i"], input);
        assert!(ok && out.is_empty(), "{input:?}: {all}");
    }
    assert!(!f.wt("abc", "a").exists());
    let (ok, out, _) = f.interactive(&a, &["list", "-i", "-o"], "\r");
    assert!(ok && out.is_empty());
    assert!(f.wait_clion(f.wt("one", "a").to_str().unwrap()));

    // n: a new worktree branched from the highlighted checkout (Backspace edits the name).
    let (ok, out, all) = f.interactive(&a, &["list", "-i"], "jnfeatX\x7f\r");
    let feat = f.wt("feat", "a");
    assert!(ok && out.trim() == feat.to_str().unwrap(), "{all}");
    assert!(all.contains("Creating new branch 'feat' from two"), "{all}");
    assert_eq!(git(&feat, &["rev-parse", "HEAD"]), git(&two, &["rev-parse", "HEAD"]));
    let (ok, out, all) = f.interactive(&f.root, &["list", "-A", "-r", "-i"], "\x1b[Bnfrom-b\r");
    assert!(ok && out.trim() == f.wt("from-b", "b").to_str().unwrap(), "{all}");
    assert!(all.contains("Creating new branch 'from-b' from main") && f.wt("from-b", "b").join(".git").exists(), "{all}");

    // r / d / Delete ask first; y removes the worktree and the menu comes back; main repos can't be removed.
    fs::write(feat.join("junk"), "x").unwrap();
    let (ok, out, all) = f.interactive(&a, &["list", "-i"], "dn\x1b[3~q");
    assert!(ok && out.is_empty() && feat.is_dir(), "{all}");
    assert!(all.contains("Remove worktree feat/a [dirty]") && all.matches("[y/N]").count() == 2, "{all}");
    let (ok, out, all) = f.interactive(&a, &["list", "-i", "-r"], "d\r");
    assert!(ok && out.trim() == a.to_str().unwrap() && !all.contains("[y/N]"), "{all}");
    let (ok, out, all) = f.interactive(&a, &["list", "-i"], "ry\r");
    assert!(ok && !feat.exists() && out.trim() == f.wt("one", "a").to_str().unwrap(), "next row is highlighted after removal: {all}");
    let (ok, out, all) = f.interactive(&two, &["list", "-i"], "ry");
    assert!(ok && !two.exists() && out.trim() == a.to_str().unwrap(), "removing the current worktree lands in main: {all}");
    assert!(all.contains("(you'll land in the main repo)"), "{all}");
}

#[test]
fn list_and_completion_filter_by_repo_and_roots_are_independent() {
    let f = Fixture::new();
    let a = f.repo("a");
    let b = f.repo("b");
    f.init();
    f.ok(&a, &["only-a"]);
    f.ok(&b, &["only-b"]);
    let la = f.ok(&f.wt("only-a", "a"), &["list"]);
    assert!(la.contains("(repo: a)") && la.contains("only-a") && !la.contains("only-b"), "{la}");
    assert_eq!(f.ok(&a, &["__complete", "worktrees"]).trim(), "only-a");
    assert_eq!(f.ok(&f.root, &["__complete", "all-worktrees"]).trim(), "only-a\nonly-b");
    assert_eq!(f.ok(&f.root, &["__complete", "branches"]).trim(), "", "no repo at the root itself");
    assert_eq!(f.ok(&f.root, &["__complete", "bogus"]).trim(), "");

    // A second twigged directory next to the first is fully independent.
    let other = f.tmp.path().join("Other");
    let c = other.join("c");
    fs::create_dir_all(&c).unwrap();
    git(&c, &["init", "-q", "-b", "main"]);
    git(&c, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"]);
    f.ok(&other, &["init", "wts"]);
    f.ok(&c, &["z"]);
    assert!(other.join("wts/z/c").is_dir());
    assert!(f.ok(&c, &["status"]).contains("with 1 git repositories, 1 worktrees (wts)"));
    assert!(f.ok(&a, &["status"]).contains("with 2 git repositories, 2 worktrees (.WORKTREES)"));
    assert!(f.fail(&c, &["open", "a"]).contains("No repo"));

    // Exit from the main repo itself; outside any repo it fails.
    assert!(f.ok(&a, &["exit"]).contains("Already in the main repo"));
    assert!(f.fail(&f.root, &["exit"]).contains("not inside a git repository"));

    // open: explicit absolute and relative paths, and ambiguity.
    for arg in [a.to_str().unwrap(), "./a"] {
        let o = f.run(&f.root, &["open", arg]);
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), a.to_str().unwrap(), "open {arg}");
    }
    fs::create_dir_all(f.root.join("ab/.git")).unwrap();
    assert!(f.fail(&f.root, &["open", "A"]).contains("Ambiguous"));
}
