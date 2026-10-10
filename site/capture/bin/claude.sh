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
printf '\033[2J\033[H'
printf '%s╭────────────────────────────────────────────────────────────╮%s\n' "$m" "$r"
printf '%s│%s%s agent session%s%s · %-43s%s%s│%s\n' "$m" "$r" "$b" "$r" "$d" "claude${*:+ $*}" "$r" "$m" "$r"
printf '%s╰────────────────────────────────────────────────────────────╯%s\n\n' "$m" "$r"
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
case "${CC_AGENT_STATE:-idle}" in
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
    while printf '\033[1;36m>\033[0m ' && IFS= read -r line; do
      printf '\n'
      state busy
      steps 482 | sed "s/^●/${m}●${r}/"
      printf '\n%s⠋ running cargo test…%s  %sesc to interrupt%s\n' "$m" "$r" "$d" "$r"
      while :; do sleep 1; done
    done
    ;;
esac
