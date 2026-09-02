mod add;
mod cli;
mod color;
mod config;
mod error;
mod git;
mod hook;
mod idea;
mod init;
mod launcher;
mod list;
mod menu;
mod open;
mod out;
mod prune;
mod remove;
mod root;
mod shell;
mod status;
mod tint;

use clap::Parser;
use cli::{BranchArgs, Cli, Cmd, Color};
use error::Result;

fn run(cli: Cli) -> Result<()> {
    let mut g = cli.globals;
    let cmd = match cli.cmd {
        Some(Cmd::Branch(words)) if words.iter().any(|w| w == "-h" || w == "--help") => {
            Cli::parse_from(["twig", "--help"]);
            return Ok(());
        }
        Some(Cmd::Branch(words)) => {
            let args = BranchArgs::try_parse_from(std::iter::once("twig".to_string()).chain(words)).unwrap_or_else(|e| e.exit());
            g.open |= args.globals.open;
            g.color = args.globals.color.or(g.color);
            Some(Cmd::Branch(vec![args.branch].into_iter().chain(args.base).collect()))
        }
        cmd => cmd,
    };
    out::set_color(g.color.unwrap_or(Color::Auto));
    match cmd {
        None if g.open => open::current(),
        None => {
            Cli::parse_from(["twig", "--help"]);
            Ok(())
        }
        Some(Cmd::Branch(words)) => add::run(&words[0], words.get(1).map(String::as_str), g.open),
        Some(Cmd::Init(args)) => init::run(args),
        Some(Cmd::Status) => init::status(),
        Some(Cmd::List(args)) => list::run(args, g.open),
        Some(Cmd::Open { name }) => open::run(name.as_deref(), g.open),
        Some(Cmd::Exit {}) => open::exit(g.open),
        Some(Cmd::Remove { target, repo, list }) => remove::run(target.as_deref(), repo.as_deref(), list, g.open),
        Some(Cmd::Prune(args)) => prune::run(args, g.open),
        Some(Cmd::Shell { shell }) => shell::run(&shell),
        Some(Cmd::Complete { kind }) => shell::complete(&kind),
        Some(Cmd::BgRemove { worktree, branch_dir, main }) => remove::bg_remove(&worktree, &branch_dir, &main),
    }
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
