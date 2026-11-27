# alash shell integration for zsh. Load with: eval "$(alash init zsh)"
# Only what a child process cannot do lives here: the previous command, the exit status
# and the command line. Everything else is in the alash binary.
#
# `function name {` and not `name() {`: the aliases at the end are named ask and why, and
# on a second eval in the same shell `ask() {` would be expanded through the alias.
function ask {
  if [[ $1 == --doctor ]]; then
    # Evidence for the doctor: is `why` still ours, and is the key still bound to our widget?
    # ALASH_INIT is the alash version that printed this code, so the doctor can tell a
    # shell that started before an upgrade.
    local w=0 k=0
    [[ $functions[why] == *'alash why'* ]] && w=1
    [[ "$(bindkey '^X^A' 2>/dev/null)" == *' _alash_cmd' ]] && k=1
    ALASH_SHELL=zsh ALASH_INIT=@VERSION@ ALASH_WHY=$w ALASH_KEY=$k COLUMNS=$COLUMNS command alash ask "$@"
  else
    ALASH_SHELL=zsh command alash ask "$@"
  fi
}
function why { local s=$?; ALASH_SHELL=zsh command alash why "$s" "$(fc -ln -1)" "$@" </dev/null }
_alash_cmd() {
  if [[ -z ${BUFFER//[[:space:]]/} ]]; then
    zle -M "Type what the command should do, then Ctrl-X Ctrl-A."
    return
  fi
  zle -R "asking..."
  local out
  if out=$(ALASH_SHELL=zsh command alash cmd "$BUFFER" </dev/null 2>&1); then
    # First line: the command. Any lines after it: a warning from the harness.
    local note= msg=
    [[ $out == *$'\n'* ]] && note=${out#*$'\n'} out=${out%%$'\n'*}
    BUFFER=$out
    CURSOR=${#BUFFER}
    local first=${${(z)out}[1]}
    [[ $first == *=* ]] || whence -- "$first" >/dev/null || msg="not installed: $first"
    # The same warning is shown once per shell session, not at every key press.
    if [[ -n $note && $note != $_alash_note ]]; then
      typeset -g _alash_note=$note
      msg=${msg:+$msg$'\n'}$note
    fi
    # No `zle redisplay` after this: zle redraws when the widget returns, and an explicit
    # redisplay here drew the prompt one row up, over the last line of previous output,
    # and wiped the message. scripts/check-keybinding.sh guards it.
    zle -M "$msg"
  else
    zle -M "$out"
  fi
}
# A question ends in a question mark. Without noglob zsh reads `?`, `*` and `[...]` as
# filename patterns and refuses to run the command when nothing matches.
alias ask='noglob ask' why='noglob why'
if [[ -o interactive ]]; then
  zle -N _alash_cmd
  bindkey '^X^A' _alash_cmd
fi
