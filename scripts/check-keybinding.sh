#!/bin/sh
# Presses the key binding in a real terminal and checks what is left on screen.
#
# The fake-harness tests see the bytes alash and the shell write, not where they land.
# That let a wrong redraw ship: the prompt moved up a row and the "not installed" message
# never stayed visible. This runs zsh and bash inside a private tmux server and reads the
# screen back.
#
# usage: scripts/check-keybinding.sh <directory holding the alash binary>
# needs: tmux, zsh, bash
set -eu
bin=$(cd "$1" && pwd)
work=$(mktemp -d)
trap 'tmux -L alash-check kill-server 2>/dev/null || true; rm -r "$work"' EXIT
cat > "$work/claude" <<'FAKE'
#!/bin/sh
[ -n "${KB_WARN:-}" ] && echo "$KB_WARN" >&2
echo "${KB_REPLY:-ls -la}"
FAKE
chmod +x "$work/claude"
fail=0

# screen <shell> <reply> <warning>: prints the terminal after the key binding has run
screen() {
  t="tmux -L alash-check -f /dev/null"
  $t kill-server 2>/dev/null || true
  case $1 in
    zsh) start="zsh -f -i"; init='unsetopt PROMPT_SP; eval "$(alash init zsh)"; clear' ;;
    bash) start="bash --norc --noprofile -i"; init='PS1="> "; eval "$(alash init bash)"; clear' ;;
  esac
  $t new-session -d -x 90 -y 20 \
    "env -i HOME=$work TERM=xterm-256color PATH=$work:$bin:/usr/bin:/bin KB_REPLY='$2' KB_WARN='$3' PS1='> ' $start"
  sleep 0.6
  $t send-keys "$init" Enter
  sleep 0.6
  $t send-keys 'printf "%s\n" "earlier line" "LAST LINE OF OUTPUT"' Enter
  sleep 0.4
  $t send-keys -l 'list files'
  sleep 0.2
  $t send-keys C-x C-a
  sleep 1.5
  $t capture-pane -p
  $t kill-server
}

# expect <what> <screen> <line that must be on it>...
expect() {
  what=$1 shown=$2
  shift 2
  for line in "$@"; do
    if ! printf '%s\n' "$shown" | grep -qxF -- "$line"; then
      echo "FAIL: $what: no line '$line' on screen:"
      printf '%s\n' "$shown" | sed -n '1,8p' | sed 's/^/    |/'
      fail=1
      return
    fi
  done
  echo "ok: $what"
}

for sh in zsh bash; do
  expect "$sh places the command and leaves earlier output alone" \
    "$(screen $sh 'ls -la' '')" "LAST LINE OF OUTPUT" "> ls -la"
  expect "$sh shows a warning from the harness" \
    "$(screen $sh 'ls -la' 'Warning: sign-in ends soon.')" \
    "LAST LINE OF OUTPUT" "> ls -la" "Warning: sign-in ends soon."
  expect "$sh warns when the command is not installed" \
    "$(screen $sh 'nosuchprog-xyz --go' '')" \
    "LAST LINE OF OUTPUT" "> nosuchprog-xyz --go" "not installed: nosuchprog-xyz"
  expect "$sh shows a question and places nothing" \
    "$(screen $sh 'Which files?' '')" "LAST LINE OF OUTPUT" "> list files" "Which files?"
done
exit $fail
