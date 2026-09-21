//! Throwaway twigged directory with real git repos and bare origins; `clion` is a fake script on PATH that logs its argv.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::io::Write;

pub struct Fixture {
    pub tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub bin: PathBuf,
}

pub fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

impl Fixture {
    pub fn new() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Projects");
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let clion = bin.join("clion");
        fs::write(&clion, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", tmp.path().join("clion.log").display())).unwrap();
        fs::set_permissions(&clion, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        Fixture { tmp: tmp, root, bin }
    }

    /// A repo `<root>/<name>` with one commit on `main` and a bare `origin`.
    pub fn repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join(name);
        let origin = self.tmp.path().join(format!("{}.git", name.replace('/', "-")));
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

    pub fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_twig"));
        c.current_dir(cwd).args(args);
        c.env("PATH", format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()));
        c.env("TWIG_SHELL", "1");
        c.env("TMPDIR", self.tmp.path());
        c
    }

    pub fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd(cwd, args).output().unwrap()
    }

    pub fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.run(cwd, args);
        assert!(out.status.success(), "twig {:?} failed:\n{}{}", args, String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    }

    pub fn fail(&self, cwd: &Path, args: &[&str]) -> String {
        let out = self.run(cwd, args);
        assert!(!out.status.success(), "twig {:?} unexpectedly succeeded", args);
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    pub fn wt(&self, branch_dir: &str, repo: &str) -> PathBuf {
        self.root.join(".WORKTREES").join(branch_dir).join(repo)
    }

    /// Run with scripted stdin; returns (success, stdout, stdout+stderr).
    pub fn interactive(&self, cwd: &Path, args: &[&str], input: &str) -> (bool, String, String) {
        let mut child = self.cmd(cwd, args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let all = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        (out.status.success(), stdout, all)
    }

    pub fn clion_log(&self) -> String {
        fs::read_to_string(self.tmp.path().join("clion.log")).unwrap_or_default()
    }

    /// `clion` is spawned detached, so wait for its log line to land.
    pub fn wait_clion(&self, needle: &str) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.clion_log().contains(needle) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    pub fn init(&self) {
        self.ok(&self.root, &["init"]);
    }
}
