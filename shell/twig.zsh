# twig shell integration (zsh). Installed once, globally:  eval "$(twig shell zsh)"
# The wrapper is inert outside a twigged directory: the binary itself decides.

twig () {
  # Commands that land you somewhere print just the target path on stdout
  # (progress goes to stderr); this function cd's there. With -o/--open the
  # binary opens the IDE instead and prints nothing on stdout.
  case "$1" in
    list) __twig_list_switches "$@" || { TWIG_SHELL=1 command twig "$@"; return } ;;
    init|status|shell|help|__complete|-h|--help|-V|--version|"")
      TWIG_SHELL=1 command twig "$@"; return ;;
  esac
  local a; for a in "$@"; do
    [[ $a == -h || $a == --help ]] && { TWIG_SHELL=1 command twig "$@"; return; }
  done
  local dir
  dir=$(TWIG_SHELL=1 command twig "$@") || return
  [[ -n $dir && -d $dir ]] && cd -- "$dir"
  return 0
}

# `list` only lands somewhere with -i/--interactive-switch (also clustered: -li).
__twig_list_switches () {
  local a; for a in "$@"; do
    [[ $a == --interactive-switch || ( $a == -[!-]* && $a == *i* ) ]] && return 0
  done
  return 1
}

__twig_items () {  # $1 = kind, $2 = tag, $3 = description
  local -a items
  items=( ${(f)"$(command twig __complete "$1" 2>/dev/null)"} )
  (( ${#items} )) || return 1
  local expl
  _wanted "$2" expl "$3" compadd -a items
}

_twig () {
  local -a cmds
  cmds=(
    'init:set up twig for the current directory'
    'status:show whether twig is active here'
    'list:tree of this repo'\''s worktrees (-l: state, -r: main repos, -i: switch menu)'
    'open:cd into a repo (-o: open it in the IDE)'
    'exit:cd into the main repo (-o: open it in the IDE)'
    'remove:remove a worktree'
    'prune:remove worktrees whose branch is gone from origin'
  )
  # Global flags may precede the command; skip them so words[2] is the command.
  while (( CURRENT > 2 )) && [[ $words[2] == (-o|--open|--color|--color=*) ]]; do
    [[ $words[2] == --color ]] && { shift words; (( CURRENT-- )); }
    shift words; (( CURRENT-- ))
  done
  if (( CURRENT == 2 )); then
    _describe -t commands 'twig command' cmds
    __twig_items branches branches 'branch'
    _arguments '(-o --open)'{-o,--open}'[open the worktree in the IDE instead of cd-ing]' '--color=[colour output]:when:(auto always never)' '(- *)'{-h,--help}'[show help]'
    return
  fi
  # _arguments counts positionals from words[1], so drop the subcommand first.
  local sub=$words[2]
  shift words; (( CURRENT-- ))
  local -a color; color=( '--color=[colour output]:when:(auto always never)' )
  case $sub in
    init)   _arguments '--ide=[IDE launcher command]' '--no-tint[no editor background tint]' '--opacity=[tint opacity %]' '--saturation=[0..1]' '--lightness=[0..1]' ':worktrees folder name:' ;;
    list)   _arguments $color '(-A --all)'{-A,--all}'[every repo under the twigged directory]' '(-l --long)'{-l,--long}'[show dirty / unpushed / never pushed / gone]' \
              '(-r --root-repos)'{-r,--root-repos}'[also list the main repos]' '(-i --interactive-switch)'{-i,--interactive-switch}'[menu: arrows move, Enter switches, n new worktree, r/d/Del remove]' \
              '(-o --open)'{-o,--open}'[with -i: open the chosen checkout in the IDE]' ;;
    open)   _arguments $color '(-o --open)'{-o,--open}'[open in the IDE instead of cd-ing]' '1:repo:{__twig_items repos repos repo}' ;;
    exit)   _arguments $color '(-o --open)'{-o,--open}'[open in the IDE instead of cd-ing]' ;;
    remove) _arguments $color '(- *)'{-l,--list}'[list all generated worktrees]' '(-o --open)'{-o,--open}'[open the main repo in the IDE]' \
              '1:worktree:{__twig_items all-worktrees worktrees worktree}' '2:repo:{__twig_items repos repos repo}' ;;
    prune)  _arguments $color '(-A --all)'{-A,--all}'[every repo]' '(-R --skip-local)'{-R,--skip-local}'[skip never-pushed]' \
              '(-C --skip-dirty)'{-C,--skip-dirty}'[skip dirty / unpushed]' '(-q --query)'{-q,--query}'[ask origin instead of trusting the last fetch]' \
              '(-o --open)'{-o,--open}'[open the main repo in the IDE]' ;;
    status) ;;
    *) __twig_items branches branches 'base ref' ;;
  esac
}
compdef _twig twig
