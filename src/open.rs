//! `twig open` / `twig exit`.
use crate::error::Result;
use crate::root::Root;
use crate::{add, bail, git, out};
use std::path::Path;

pub fn run(name: Option<&str>, open: bool) -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    let Some(name) = name else {
        out::say(format!("Repos under {}:", root.dir.display()));
        for r in root.repos() {
            out::say(format!("  {}", root.repo_name(&r)));
        }
        return Ok(());
    };
    // A name with a slash that exists relative to the cwd is an explicit path.
    let p = Path::new(name);
    let target = if name.contains('/') && p.is_dir() { p.canonicalize()? } else { root.resolve_repo(name)? };
    add::land(&root, &target, open);
    Ok(())
}

pub fn exit(open: bool) -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    let cwd = std::env::current_dir()?;
    let main = root.main_repo_at(&cwd)?;
    if git::toplevel(&cwd).as_deref() == Some(main.as_path()) {
        out::say(format!("Already in the main repo: {}", main.display()));
    } else {
        out::say(format!("Main repo: {}", main.display()));
    }
    add::land(&root, &main, open);
    Ok(())
}


/// `twig -o` without a branch: open the checkout you're in (worktree or main repo).
pub fn current() -> Result<()> {
    out::set_terminal_mode();
    let root = Root::discover()?;
    let cwd = std::env::current_dir()?;
    let Some(top) = git::toplevel(&cwd) else { bail!("Not inside a git repo; pass a branch or repo to open.") };
    add::land(&root, &top, true);
    Ok(())
}
