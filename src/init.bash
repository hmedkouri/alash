# alash shell integration for bash. Load with: eval "$(alash init bash)"
# Only what a child process cannot do lives here: the previous command, the exit status
# and the command line. Everything else is in the alash binary.
ask() { ALASH_SHELL=bash command alash ask "$@"; }
why() { local s=$?; ALASH_SHELL=bash command alash why "$s" "$(fc -ln -1)" "$@" </dev/null; }
_alash_cmd() {
  if [[ -z ${READLINE_LINE//[[:space:]]/} ]]; then
    printf '%s\n' "Type what the command should do, then Ctrl-X Ctrl-A."
    return
  fi
  printf 'asking...\r'
  local out first
  if out=$(ALASH_SHELL=bash command alash cmd "$READLINE_LINE" </dev/null 2>&1); then
    printf '\033[K'
    READLINE_LINE=$out
    READLINE_POINT=${#out}
    first=${out%%[[:space:]]*}
    [[ $first == *=* ]] || type -t -- "$first" >/dev/null || printf 'not installed: %s\n' "$first"
  else
    printf '\033[K%s\n' "$out"
  fi
}
if [[ $- == *i* ]]; then
  bind -x '"\C-x\C-a": _alash_cmd'
fi
