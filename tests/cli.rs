//! End-to-end tests against a throwaway twigged directory with real git repos.
//! `clion` is a fake script on PATH that logs its argv.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::io::Write;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    bin: PathBuf,
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

impl Fixture {
    fn new() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Projects");
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let clion = bin.join("clion");
        fs::write(&clion, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", tmp.path().join("clion.log").display())).unwrap();
        fs::set_permissions(&clion, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        Fixture { _tmp: tmp, root, bin }
    }

    /// A repo `<root>/<name>` with one commit on `main` and a bare `origin`.
    fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        let origin = self._tmp.path().join(format!("{}.git", name.replace('/', "-")));
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        fs::write(repo.join("README"), name).unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        git(origin.parent().unwrap(), &["init", "-q", "--bare", origin.to_str().unwrap()]);
        git(&repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&repo, &["push", "-q", "-u", "origin", "main"]);
        repo
    }

    fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_twig"));
        c.current_dir(cwd).args(args);
        c.env("PATH", format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()));
        c.env("TWIG_SHELL", "1");
        c.env("TMPDIR", self._tmp.path());
        c
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd(cwd, args).output().unwrap()
    }

    fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.run(cwd, args);
        assert!(out.status.success(), "twig {:?} failed:\n{}{}", args, String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    }

    fn fail(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.run(cwd, args);
        assert!(!out.status.success(), "twig {:?} unexpectedly succeeded", args);
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    fn wt(&self, branch_dir: &str, repo: &str) -> PathBuf {
        self.root.join(".WORKTREES").join(branch_dir).join(repo)
    }

    /// Run with scripted stdin; returns (success, stdout, stdout+stderr).
    fn interactive(&self, cwd: &Path, args: &[&str], input: &str) -> (bool, String, String) {
        let mut child = self.cmd(cwd, args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let all = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        (out.status.success(), stdout, all)
    }

    fn clion_log(&self) -> String {
        fs::read_to_string(self._tmp.path().join("clion.log")).unwrap_or_default()
    }

    /// `clion` is spawned detached, so wait for its log line to land.
    fn wait_clion(&self, needle: &str) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.clion_log().contains(needle) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    fn init(&self) {
        self.ok(&self.root, &["init"]);
    }
}

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
    let outer = f._tmp.path();
    assert!(f.fail(outer, &["init"]).contains("nested"));
    let warn = String::from_utf8_lossy(&f.cmd(&f.root, &["status"]).env_remove("TWIG_SHELL").output().unwrap().stderr).to_string();
    assert!(warn.contains("shell integration not active"), "{warn}");
}

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
    fs::write(&myide, format!("#!/bin/sh\necho \"MYIDE $@\" >> '{}'\n", f._tmp.path().join("clion.log").display())).unwrap();
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
    let lib_origin = f._tmp.path().join("lib.git");
    git(&app, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", lib_origin.to_str().unwrap(), "libs/lib"]);
    git(&app, &["commit", "-qm", "submodule"]);
    f.init();
    // Make the origin unreachable: seeding must work from local objects alone.
    fs::rename(&lib_origin, f._tmp.path().join("lib.git.moved")).unwrap();
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
    let origin = f._tmp.path().join("a.git");
    git(&origin, &["update-ref", "-d", "refs/heads/elsewhere"]);
    let out = f.ok(&a, &["prune", "-C"]);
    assert!(out.contains("Nothing to prune for a"), "{out}");
    let (_, _, err) = f.interactive(&a, &["prune", "-C", "-q"], "\nn\n");
    assert!(err.contains("Querying origin of a") && err.contains("elsewhere/a") && err.contains("[gone]"), "{err}");
    // Unreachable origin with -q skips b's worktrees with a warning.
    fs::remove_dir_all(f._tmp.path().join("b.git")).unwrap();
    let out = f.ok(&b, &["prune", "-q"]);
    assert!(out.contains("cannot query origin for b") && out.contains("Nothing to prune for b"), "{out}");
}

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

    // Colour: auto is off when piped; always/never override; the flag goes after the subcommand.
    assert!(!plain.contains("\x1b["));
    assert!(f.ok(&work, &["list", "--color=always"]).contains("\x1b[92m← here\x1b[0m"));
    assert!(!f.ok(&work, &["list", "--color=never"]).contains("\x1b["));
    assert!(!f.ok(&work, &["list", "--color=none"]).contains("\x1b["));
}

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

    for (cmd, flag) in [("list", "--long"), ("open", "--open"), ("exit", "--open"), ("remove", "--list"), ("prune", "--query"), ("init", "--ide"), ("status", "--color")] {
        let help = f.ok(&f.root, &[cmd, "--help"]);
        assert!(help.contains("Usage: twig ") && help.contains(flag), "{cmd}: {help}");
        assert_eq!(f.ok(&f.root, &["-o", cmd, "--help"]), help, "-o before {cmd}");
    }
    let help = f.ok(&f.root, &["open", "--help"]);
    assert!(help.contains("[NAME]  Repo:"), "{help}");
    let help = f.ok(&f.root, &["list", "--help"]);
    assert!(help.contains("-r, --root-repos") && help.contains("-i, --interactive-switch"), "{help}");
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
    let other = f._tmp.path().join("Other");
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
