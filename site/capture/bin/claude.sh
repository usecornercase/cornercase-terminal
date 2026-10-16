#!/bin/bash
dir=$CLAUDE_CONFIG_DIR
mkdir -p "$dir/sessions"
slug=$(printf '%s' "$PWD" | tr -c 'a-zA-Z0-9' '-')
mkdir -p "$dir/projects/$slug"
if [ "${1:-}" = -p ]; then
  read -r _init
  read -r _usage
  printf '{"type":"system"}\n'
  cat "$CC_DATA/usage/claude.json"
  printf '\n'
  exec sleep 30
fi
pct=${CC_AGENT_PERCENT:-7}
model=${CC_AGENT_MODEL:-claude-opus-5-5}
read_tokens=$((pct * 10000 - 15657))
printf '{"type":"assistant","message":{"model":"%s","usage":{"input_tokens":2,"cache_creation_input_tokens":15655,"cache_read_input_tokens":%s}}}\n' "$model" "$read_tokens" > "$dir/projects/$slug/s$$.jsonl"
state() {
  printf '{"pid":%s,"sessionId":"s%s","cwd":"%s","status":"%s"}' $$ $$ "$PWD" "$1" > "$dir/sessions/$$.json"
}
trap 'rm -f "$dir/sessions/$$.json"; exit 0' INT TERM HUP
m=$(printf '\033[35m'); b=$(printf '\033[1m'); d=$(printf '\033[2m'); r=$(printf '\033[0m')
args="$*"
box() {
  printf '%s╭────────────────────────────────────────────────────────────╮%s\n' "$m" "$r"
  printf '%s│%s%s agent session%s%s · %-43s%s%s│%s\n' "$m" "$r" "$b" "$r" "$d" "claude${args:+ $args}" "$r" "$m" "$r"
  printf '%s╰────────────────────────────────────────────────────────────╯%s\n\n' "$m" "$r"
}
printf '\033[2J\033[H'
box
steps() {
  case "$1" in
    482) printf '● Reading src/returns/address.rs\n● first_line() panics on an empty address\n● Returning Option<&str>, answering 422\n● Adding a test: empty_address_is_rejected\n' ;;
    dark) printf '● Reading src/theme.rs and the templates\n● The checkout hardcodes #ffffff 3 times\n● Using Theme::background() everywhere\n' ;;
    pagination) printf '● Reading src/orders.ts and the routes\n● GET /orders returns every order at once\n● Adding a cursor and a limit of 50\n' ;;
    *) printf '● Reading the issue and the project\n● Planning the change in three steps\n' ;;
  esac
}
prompt() {
  case "$1" in
    482) printf '> https://github.com/acme/web-shop/issues/482\n\n' ;;
    dark) printf '> https://github.com/acme/web-shop/issues/479\n\n' ;;
    pagination) printf '> Paginate GET /orders with a cursor\n\n' ;;
    *) printf '> %s\n\n' "$1" ;;
  esac
}
fresh() {
  stty -echo 2>/dev/null
  printf '\033[2J\033[H'
  box "$@"
}
drain() {
  printf '%s\n' "$1"
  while IFS= read -r -t 1 more; do printf '%s\n' "$more"; done
}
said() {
  printf '%s●%s %s\n' "$m" "$r" "$1"
}
member() {
  role=$(drain "$1" | sed -n 's/.*You are `\([^`]*\)` in this team.*/\1/p' | head -n 1)
  fresh
  printf '> You are %s in a team of coding agents\n\n' "$role"
  said 'Waiting for the orchestrator'
  printf '\n'
  while printf '\033[1;36m>\033[0m ' && IFS= read -r line; do
    [ -n "$line" ] || continue
    drain "$line" >/dev/null
    printf '%s\n\n' "${line:0:56}"
    state busy
    case "$role:$line" in
      coder:Fix*) said 'Reading src/returns/address.rs'; said 'Returning Option<&str>, answering 422' ;;
      coder:*) said 'Adding the test the reviewers asked for' ;;
      security:*) said 'Reading the diff against main' ;;
      *) said 'Working on it' ;;
    esac
    sleep 2
    case "$role:$line" in
      coder:*) said 'Committed on this branch, cargo test passes' ;;
      security:*) said 'No findings.' ;;
      *) said 'Done.' ;;
    esac
    printf '\n'
    state idle
  done
  exit 0
}
orchestrate() {
  brief=$(drain "$1")
  cc="$CC_CAPTURE/../../target/debug/cornercase"
  pane() {
    printf '%s\n' "$brief" | grep -o -- "- $1 (pane [0-9]*)" | head -n 1 | tr -dc '0-9'
  }
  tell() {
    "$cc" send --pane "$(pane "$1")" "$2"$'\n' >/dev/null
  }
  fresh
  printf '> You are the orchestrator of a team of coding agents\n\n'
  state busy
  said 'Sending issue #482 to coder'
  tell coder 'Fix #482: the returns page crashes on an empty address.'
  sleep 3
  said 'coder committed the fix: asking reviewer and security'
  tell reviewer 'Review the fix for #482 on this branch.'
  tell security 'Check the fix for #482 for security problems.'
  sleep 3
  said 'reviewer found 1 problem: back to coder (round 1 of 3)'
  tell coder 'reviewer: 1. there is no test for an empty address.'
  sleep 3
  said 'coder added the test: asking reviewer to check again'
  tell reviewer 'Check the fix for #482 again: coder added the test.'
  sleep 2
  printf '\n%s⠋ waiting for reviewer…%s\n' "$m" "$r"
  while :; do sleep 1; done
}
case "${CC_AGENT_STATE:-idle}" in
  failed)
    printf '\033[31m! Failed to complete the task: authentication expired.\033[0m\n'
    state idle
    printf '\033]7501;state=error:app=claude-code\033\\'
    while :; do sleep 1; done
    ;;
  busy)
    prompt "${CC_AGENT_TASK:-482}"
    steps "${CC_AGENT_TASK:-482}" | sed "s/^●/${m}●${r}/"
    printf '\n%s⠋ running cargo test…%s  %sesc to interrupt%s\n' "$m" "$r" "$d" "$r"
    state busy
    while :; do sleep 1; done
    ;;
  waiting)
    prompt "${CC_AGENT_TASK:-pagination}"
    steps "${CC_AGENT_TASK:-pagination}" | sed "s/^●/${m}●${r}/"
    printf '\n\033[33m●\033[0m %sEdit%s src/orders.ts\n  %sDo you want to make this edit?%s\n' "$b" "$r" "$b" "$r"
    printf '\033[36m❯ 1. Yes\033[0m\n  2. Yes, and don’t ask again this session\n  3. No, and tell Claude what to do\n'
    state waiting
    while :; do sleep 1; done
    ;;
  *)
    printf '%stip: describe a change or paste a link%s\n\n' "$d" "$r"
    state idle
    printf '\033[1;36m>\033[0m '
    IFS= read -r line || exit 0
    case "$line" in
      'You are the orchestrator of a team'*) orchestrate "$line" ;;
      *'in a team of coding agents'*) member "$line" ;;
    esac
    printf '\n'
    state busy
    steps 482 | sed "s/^●/${m}●${r}/"
    printf '\n%s⠋ running cargo test…%s  %sesc to interrupt%s\n' "$m" "$r" "$d" "$r"
    while :; do sleep 1; done
    ;;
esac
