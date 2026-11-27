# alash shell integration for bash. Load with: eval "$(alash init bash)"
# Only what a child process cannot do lives here: the previous command, the exit status
# and the command line. Everything else is in the alash binary.
ask() {
  if [[ $1 == --doctor ]]; then
    # Evidence for the doctor: is `why` still ours, and is the key still bound to our widget?
    local w=0 k=0
    [[ "$(declare -f why)" == *'alash why'* ]] && w=1
    [[ "$(bind -X 2>/dev/null)" == *'"\C-x\C-a": "_alash_cmd"'* ]] && k=1
    ALASH_SHELL=bash ALASH_WHY=$w ALASH_KEY=$k COLUMNS=$COLUMNS command alash ask "$@"
  else
    ALASH_SHELL=bash command alash ask "$@"
  fi
}
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
    # First line: the command. Any lines after it: a warning from the harness.
    local note=
    [[ $out == *$'\n'* ]] && note=${out#*$'\n'} out=${out%%$'\n'*}
    READLINE_LINE=$out
    READLINE_POINT=${#out}
    first=${out%%[[:space:]]*}
    [[ $first == *=* ]] || type -t -- "$first" >/dev/null || printf 'not installed: %s\n' "$first"
    # The same warning is shown once per shell session, not at every key press.
    if [[ -n $note && $note != "$_alash_note" ]]; then
      _alash_note=$note
      printf '%s\n' "$note"
    fi
  else
    printf '\033[K%s\n' "$out"
  fi
}
if [[ $- == *i* ]]; then
  bind -x '"\C-x\C-a": _alash_cmd'
fi
