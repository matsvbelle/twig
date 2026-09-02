use clap::{Args, Parser, Subcommand, ValueEnum};

/// One worktree per branch, one IDE window per worktree.
#[derive(Parser, Debug)]
#[command(name = "twig", version, allow_external_subcommands = true)]
#[command(override_usage = "twig [OPTIONS] [BRANCH] [BASE]\n       twig [OPTIONS] <COMMAND>")]
#[command(after_help = "Arguments:\n  [BRANCH]  Branch to create a worktree for (or switch to its existing worktree)\n  [BASE]    Base ref for a NEW branch (default: HEAD)")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,

    #[command(flatten)]
    pub globals: Globals,
}

/// Flags accepted anywhere on the command line, before or after the command / branch.
#[derive(Args, Debug)]
pub struct Globals {
    /// Open in the IDE instead of cd-ing (no command or branch: open the current checkout)
    #[arg(short = 'o', long, global = true)]
    pub open: bool,

    /// Colour output: auto (when writing to a terminal), always, never/none
    #[arg(long, global = true, value_name = "WHEN")]
    pub color: Option<Color>,
}

/// `twig <branch> [base]`: the words clap handed us as an unknown command, re-parsed.
#[derive(Parser, Debug)]
#[command(name = "twig", disable_help_flag = true)]
pub struct BranchArgs {
    pub branch: String,
    pub base: Option<String>,
    #[command(flatten)]
    pub globals: Globals,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Auto,
    Always,
    Never,
    #[value(alias = "none", hide = true)]
    None,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Set up twig for the current directory (creates `.twig.toml` + the worktrees folder)
    Init(InitArgs),
    /// Show whether twig is active here and what it manages
    Status,
    /// Tree of this repo's worktrees; the one you're in is marked
    List(ListArgs),
    /// cd into a repo under the twigged directory (no name: list repos)
    Open {
        /// Repo: top-level name, root-relative path (e.g. `libs/core`), unique basename or substring
        name: Option<String>,
    },
    /// Leave the worktree: cd into its main repo
    Exit {
    },
    /// Remove the worktree you're in (landing in its main repo), or the given <path> | <branch> [repo]
    Remove {
        /// Worktree path, or the branch whose worktrees to remove
        target: Option<String>,
        /// With a branch: only this repo's worktree
        repo: Option<String>,
        /// List all generated worktrees
        #[arg(short = 'l', long)]
        list: bool,
    },
    /// Interactively remove worktrees whose branch is gone from origin (as last fetched; -q asks origin)
    Prune(PruneArgs),
    /// Print the zsh integration (cd + completion); `eval "$(twig shell zsh)"`
    #[command(hide = true)]
    Shell { shell: String },
    /// Anything that isn't a command is `<branch> [base]`
    #[command(external_subcommand)]
    Branch(Vec<String>),
    /// Completion candidates for the shell integration
    #[command(name = "__complete", hide = true)]
    Complete { kind: String },
    /// Detached removal worker spawned by `remove` (never call directly)
    #[command(name = "__bg-remove", hide = true)]
    BgRemove { worktree: String, branch_dir: String, main: String },
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Name of the worktrees folder
    #[arg(default_value = crate::config::DEFAULT_WORKTREES)]
    pub name: String,
    /// IDE launcher command used by -o (e.g. `clion`, `idea`, `code`)
    #[arg(long)]
    pub ide: Option<String>,
    /// Don't tint the editor background of new worktrees
    #[arg(long, conflicts_with_all = ["opacity", "saturation", "lightness"])]
    pub no_tint: bool,
    /// Tint opacity in percent (higher = more tint)
    #[arg(long)]
    pub opacity: Option<u32>,
    /// Tint colour saturation (0..1)
    #[arg(long)]
    pub saturation: Option<f64>,
    /// Tint colour lightness (0..1)
    #[arg(long)]
    pub lightness: Option<f64>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Every repo under the twigged directory
    #[arg(short = 'A', long)]
    pub all: bool,
    /// Show each worktree's state: dirty, unpushed commits, never pushed, upstream gone (no network)
    #[arg(short = 'l', long)]
    pub long: bool,
    /// Also list the main repos, with the worktrees folder nested below them
    #[arg(short = 'r', long)]
    pub root_repos: bool,
    /// Menu: arrows move, Enter/Space cd's into the highlighted checkout (-o: opens it), n creates a worktree from it, r/d/Del removes it, q/Esc quits
    #[arg(short = 'i', long)]
    pub interactive_switch: bool,
}

#[derive(Args, Debug)]
pub struct PruneArgs {
    /// Candidates for every repo under the twigged directory
    #[arg(short = 'A', long)]
    pub all: bool,
    /// Only branches that WERE at origin (skip never-pushed)
    #[arg(short = 'R', long)]
    pub skip_local: bool,
    /// Only clean worktrees (skip dirty / unpushed commits)
    #[arg(short = 'C', long)]
    pub skip_dirty: bool,
    /// Ask origin (git ls-remote) instead of trusting the last fetch
    #[arg(short = 'q', long)]
    pub query: bool,
}
