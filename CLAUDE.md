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
  `open`, `exit`, `remove` of the current worktree, `prune`) print exactly one
  line on stdout — the path to cd into — and everything else on stderr via
  `out::say` (terminal mode). Git passthrough output is redirected too
  (`git::passthrough`). With `-o`/`--open` nothing is printed on stdout and the
  path is opened in the configured IDE (`launcher::open_in_ide`, `config.ide`). The zsh wrapper (`shell/twig.zsh`, emitted by `twig shell zsh`)
  relies on this. `TWIG_SHELL=1` is set by the wrapper so the binary can warn
  when it's missing.
- **No network unless asked.** `list -l`/`prune` judge "gone" from the local
  `[gone]` tracking state; only `prune -q` runs `ls-remote`.
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
- Comments: one line, rationale/constraints only.

## Layout

```
src/main.rs      dispatch            src/root.rs    Root, Worktree, discovery, layout
src/cli.rs       clap definitions    src/config.rs  .twig.toml schema
src/init.rs      init + status       src/add.rs     twig <branch>
src/list.rs      twig list           src/open.rs    open + exit
src/remove.rs    remove + detached worker (__bg-remove)
src/prune.rs     prune (interactive) src/shell.rs   shell snippet + __complete
src/status.rs    per-worktree dirty/unpushed/never-pushed/gone (list -l, prune)
src/idea.rs      .idea copy/symlink, workspace.xml component/property editing
src/tint.rs      branch colour + PNG src/color.rs   ProjectColorInfo palette index
src/hook.rs      branch-pin hook (hooks/branch-pin.sh embedded)
src/git.rs, out.rs, error.rs, launcher.rs
tests/cli.rs     end-to-end tests: temp twigged dir, real git repos + bare origins,
                 fake `clion` script on PATH logging its argv
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
