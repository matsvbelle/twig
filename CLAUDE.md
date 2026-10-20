# CLAUDE.md — twig

Single-binary Rust CLI (edition 2021, Rust 1.93) for the one-worktree-per-branch
IDE workflow. Read `README.md` for the user-facing behaviour; this file is
about working on the code.

## Design rules

- **No runtime dependencies.** Only `git`, `cp`, `sudo` (last resort) and the
  configured IDE launcher are shelled out to. Image generation is in-process (`png`
  crate). Never add a python/shell helper.
- **Activation = `.twig.toml`.** `root::Root::discover()` walks up from the cwd;
  nothing else (no env vars, no global config, no git aliases) decides the root.
  All tunables live in that file and are set via `twig init` options.
- **stdout contract.** Commands that land the user somewhere (`twig <branch>`,
  `open`, `exit`, `remove` of the current worktree, `prune`, `list -i`) print exactly one
  line on stdout — the path to cd into — and everything else on stderr via
  `out::say` (terminal mode). Git passthrough output is redirected too
  (`git::passthrough`). With `-o`/`--open` nothing is printed on stdout and the
  path is opened in the configured IDE (`launcher::open_in_ide`, `config.ide`). The zsh wrapper (`shell/twig.zsh`, emitted by `twig shell zsh`)
  relies on this. `TWIG_SHELL=1` is set by the wrapper so the binary can warn
  when it's missing.
- **Terminal raw mode goes through the `libc` crate** (`menu.rs`: termios, poll,
  TIOCGWINSZ), never `stty`. The menu is drawn on stderr and erased again; when
  stdin isn't a tty keys are read as plain bytes, which is how the e2e tests script it.
- **No network unless asked.** `list -l`/`prune` judge "gone" from the local
  `[gone]` tracking state; only `prune -q` runs `ls-remote`. Status probes run
  in parallel (`Status::probe_all`) and never recurse into submodules.
- **Colour goes through `out::{green,yellow,red,bold,dim,cyan}`**, gated by
  `--color` (global clap arg, `out::set_color`); `auto` checks isatty of the
  stream `say` writes to, so wrapper-captured output stays plain.
- **GIT_* env is always cleared** (`git::command`) so twig behaves the same
  whether run from a shell, a git alias, or a hook.
- **Repo identity = path relative to the root** (`Root::repo_name`), mirrored
  under `<worktrees>/<branch>/`. Discovery (`Root::repos`) recurses without a
  depth limit, stops at a `.git`, skips hidden dirs and the worktrees folder. Name lookup is
  `Root::resolve_repo` (top-level exact → path → unique basename → substring).
- **Only ever delete below `<worktrees>/<branch>/`** (`Root::is_generated`).
  A removed worktree's local branch goes only when all its commits are on a
  remote-tracking ref, or with `-f` (`remove::delete_branch`); never a remote branch.
- Comments: one line, rationale/constraints only.

## Layout

```
src/main.rs        dispatch only        src/cli.rs   clap definitions
src/error.rs       Error + bail!
src/git/mod.rs     git subprocess wrapper (GIT_* cleared, passthrough)
src/git/hook.rs    branch-pin hook (hooks/branch-pin.sh embedded)
src/model/         the world commands act on
  config.rs        .twig.toml schema
  root.rs          Root, Worktree, discovery, layout, resolve_repo
  status.rs        per-worktree dirty/unpushed/never-pushed/gone (list -l, prune)
src/cmd/           one file per subcommand, each exposes run()
  add.rs           twig <branch>        init.rs      init + status
  list.rs          list (-r, -i)        open.rs      open + exit
  remove.rs        remove + detached worker (__bg-remove)
  prune.rs         prune (interactive)  shell.rs     shell snippet + __complete
src/ide/           everything JetBrains-specific
  idea.rs          .idea copy/symlink, workspace.xml component/property editing
  tint.rs          branch colour + PNG  color.rs     ProjectColorInfo palette index
  launcher.rs      open_in_ide
src/term/          terminal I/O
  out.rs           say, colours, --color gating (Color enum lives here)
  menu.rs          arrow-key menu for list -i (raw mode, key decoding, redraw)
tests/cli.rs       end-to-end test crate; one module per command under tests/cli/
tests/util/        fixture.rs:
                   temp twigged dir, real git repos + bare origins, fake `clion` on PATH
```

## Testing notes

- `cargo test` needs `git` on PATH; nothing else. Tests never touch a real
  a real projects directory. Manual experiments: build a mock root in a temp dir (not inside
  this repo — `init` refuses to run inside a git repo).
- `tint.rs` / `color.rs` assert fixed reference colours (sha1-derived hue /
  palette index); changing the hashing changes every existing worktree's tint.
- Detached removal is asynchronous: tests poll for the branch dir to vanish.

## Build & install

`scripts/build.sh` → `target/release/twig`; `scripts/install.sh` builds, copies
the binary to `/usr/local/bin/twig` and adds `eval "$(twig shell zsh)"` to
`$ZDOTDIR/.zshrc` once.
