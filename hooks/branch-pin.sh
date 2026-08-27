#!/usr/bin/env bash
# twig-branch-pin
# Prevent a linked git worktree from switching to a different branch. Commits,
# rebase, bisect and detached checkouts are unaffected; the main repo is exempt.
# Override once with:  WORKTREE_ALLOW_SWITCH=1 git switch <branch>
[ "$1" = "prepared" ] || exit 0
[ -n "$WORKTREE_ALLOW_SWITCH" ] && exit 0
gd=$(git rev-parse --git-dir 2>/dev/null)
case "$gd" in *"/worktrees/"*) ;; *) exit 0 ;; esac
current=$(git symbolic-ref -q HEAD)
while read -r old new ref; do
    [ "$ref" = "HEAD" ] || continue
    case "$new" in
        ref:*)
            target=${new#ref:}
            if [ -n "$current" ] && [ "$target" != "$current" ]; then
                echo "worktree-pin: this worktree is pinned to '${current#refs/heads/}'." >&2
                echo "  Refusing to switch to '${target#refs/heads/}'. Switch in the main repo," >&2
                echo "  or create a worktree:  twig '${target#refs/heads/}'" >&2
                echo "  (override once:  WORKTREE_ALLOW_SWITCH=1 git switch ...)" >&2
                exit 1
            fi
            ;;
    esac
done
exit 0
