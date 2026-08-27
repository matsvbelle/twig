//! Shell integration: a `twig()` wrapper that cd's on `-x` (a child process can
//! never change the shell's cwd) plus tab completion fed by `twig __complete`.
use crate::error::Result;
use crate::root::{find_config, Root};
use crate::{bail, git, list, out};

const ZSH: &str = include_str!("../shell/twig.zsh");

pub fn run(shell: &str) -> Result<()> {
    match shell {
        "zsh" => {
            print!("{ZSH}");
            Ok(())
        }
        other => bail!("unsupported shell '{other}' (only zsh)"),
    }
}

/// One candidate per line; silent when twig is inactive here.
pub fn complete(kind: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let Some(dir) = find_config(&cwd) else { return Ok(()) };
    let root = Root::load(&dir)?;
    let items: Vec<String> = match kind {
        "repos" => root.repos().iter().map(|r| root.repo_name(r)).collect(),
        "worktrees" => {
            let repo = list::current_repo(&root).ok();
            let mut dirs: Vec<String> = root
                .worktrees()
                .into_iter()
                .filter(|w| repo.as_deref().is_none_or(|r| w.repo == r))
                .map(|w| w.branch_dir)
                .collect();
            dirs.dedup();
            dirs
        }
        "all-worktrees" => root.worktrees().into_iter().map(|w| w.branch_dir).fold(Vec::new(), |mut v, b| {
            if v.last() != Some(&b) {
                v.push(b);
            }
            v
        }),
        "branches" => match git::main_repo(&cwd) {
            Some(main) => git::run(&main, &["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes/origin"])?
                .lines()
                .map(|b| b.strip_prefix("origin/").unwrap_or(b).to_string())
                .filter(|b| b != "HEAD")
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    };
    for item in items {
        out::say(item);
    }
    Ok(())
}
