# twig

One worktree per branch, one IDE window per worktree — as a single Rust binary
with no runtime dependencies.

![twig demo](twig-demo.gif)

## Layout

```
~/Projects/                    # a "twigged" directory (holds .twig.toml)
├── .twig.toml
├── alpha/                     # main repos, directly under the root ...
├── beta/
├── external/
│   └── beta/                  # ... or nested at any depth (hidden dirs skipped)
└── .WORKTREES/                # name configurable via `twig init <name>`
    └── <branch>/              # slashes in the branch flattened to dashes
        ├── alpha/             # linked worktree of alpha on <branch>
        ├── beta/
        └── external/beta/     # nested repos mirror their path under the root
```

A repo is identified by its path relative to the root. Where a bare name refers
to a repo (`twig open <name>`, `twig remove <branch> <name>`): an exact top-level
name wins, then an exact relative path (`external/beta`), then a unique nested
basename, then a unique case-insensitive substring.

`twig` is installed globally but only works inside a twigged directory: every
command walks up from the cwd to the nearest `.twig.toml`. Main repos and
worktrees both live under the root, so twig is active in both. Several twigged
directories can coexist; they cannot be nested.

## Install (once)

```sh
./scripts/install.sh    # builds release, copies to /usr/local/bin/twig,
                        # adds one line to ~/.zshrc:
                        #   eval "$(twig shell zsh)"   (cd support, tab completion)
```

Open a new shell afterwards. Then, per directory of repos:

```sh
cd ~/Projects
twig init                       # creates .twig.toml + .WORKTREES/
twig init trees --opacity 10    # custom worktrees folder name / tint options
twig init --ide idea            # IDE launcher used by -o (default: clion)
twig init --no-tint             # (re-running on an active dir updates ide/tint options)
twig status
```

`init` refuses to run inside a git repo or in/around an already twigged dir, and
warns when the shell integration isn't active.

## Commands

Every command that targets a directory `cd`s there by default (via the shell
wrapper); `-o` / `--open` opens it in the configured IDE instead. `twig -o` on
its own opens the checkout you're in (worktree or main repo).

Output is coloured when written to a terminal; `--color=always|never` overrides.
Like `-o`, it may go before or after the command.

| Command | |
|---|---|
| `twig <branch> [base] [-o]` | create the worktree (local branch → checkout; on origin → tracking; else new from `base`, default HEAD) and cd into it; if it exists, just go there |
| `twig list [-A] [-l] [-r] [-i] [-o]` | tree of this repo's worktrees, the current one marked (`-A`: all repos; `-l`: dirty / unpushed / never pushed / upstream gone; `-r`: the main repos too, with the worktrees folder nested below them; `-i`: menu — ↑/↓ move, Enter/Space cd's into the highlighted checkout (`-o`: opens it), `n` creates a worktree branched from it, `r`/`d`/Del removes it after confirming, `q`/Esc quits) |
| `twig exit [-o]` | cd to the main repo |
| `twig remove [-o]` | remove the worktree you're in and land in its main repo (`-o`: detached; the IDE switches to main) |
| `twig remove <path>` / `<branch> [repo]` / `-l` | remove others / list |
| `twig prune [-A] [-R] [-C] [-q] [-o]` | interactive: worktrees whose branch is gone from origin, or never pushed (`-R` skips those, `-C` skips dirty/unpushed) |
| `twig open [name] [-o]` | cd to a repo under the root; no name lists repos |
| `twig status` | active for which dir, how many repos/worktrees, ide and tint settings |

A branch named like a subcommand: `twig -- list`.

`list -l` and `prune` never touch the network: "gone" means the branch has an
upstream whose remote-tracking ref no longer exists (`[gone]` in `git branch
-vv`), i.e. as of your last `git fetch --prune`. `prune -q` asks origin
(`git ls-remote`) instead.

Each new worktree gets: `.idea` copied with `codeStyles`/`inspectionProfiles`/
`dictionaries` symlinked to the main repo; a per-branch editor background tint
(`.idea/worktree-bg.png` + `idea.background.editor`, editor/tool windows only);
the main repo's CLion project colour (initialised once per repo, avoiding
siblings' palette indices); `venv`, `.venv`, `.gitlab_token`,
`.git-blame-ignore-revs` symlinked; submodules seeded from the main repo's object
store (no re-clone); a `reference-transaction` hook pinning worktrees to their
branch (override once: `WORKTREE_ALLOW_SWITCH=1 git switch …`).

`remove`/`prune` delete the directory directly (submodule worktrees can't be
dropped by `git worktree remove`; Docker may leave root-owned files → sudo only
then), wait for the IDE to stop recreating `.idea`, then `git worktree prune`.
Branches are kept.

## `.twig.toml`

```toml
worktrees = ".WORKTREES"
ide = "clion"     # launcher command for -o; the directory is appended
[tint]            # omit the table for no tint
opacity = 7       # percent; higher = more tint
saturation = 0.55
lightness = 0.55
```

## Adopting existing worktrees

Worktrees already laid out as `<worktrees>/<branch>/<repo path>` are picked up
as-is after `twig init`; nothing needs to be re-created.

## Development

```sh
cargo test          # unit tests + end-to-end tests on temp git repos (fake `clion` on PATH)
cargo clippy --all-targets
```
