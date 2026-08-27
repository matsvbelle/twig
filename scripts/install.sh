#!/usr/bin/env bash
set -e
SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
"$SCRIPT_DIR/build.sh"
sudo install -m 755 "$SCRIPT_DIR/../target/release/twig" /usr/local/bin/twig
ls -l /usr/local/bin/twig

# Shell integration (cd on -x + completion): one idempotent line in .zshrc.
RC="${ZDOTDIR:-$HOME}/.zshrc"
LINE='eval "$(twig shell zsh)"'
if [ -f "$RC" ] && grep -qF "$LINE" "$RC"; then
    echo "Shell integration already in $RC"
else
    printf '\n# twig: cd on -x + tab completion\n%s\n' "$LINE" >> "$RC"
    echo "Added shell integration to $RC (open a new shell to pick it up)"
fi
