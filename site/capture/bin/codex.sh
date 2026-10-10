#!/bin/bash
if [ "${1:-}" = app-server ]; then
  read -r _init
  read -r _initialized
  read -r _limits
  cat "$CC_DATA/usage/codex.json"
  printf '\n'
  exec sleep 30
fi
state="$CODEX_HOME/app-server-daemon"
mkdir -p "$state" "$CODEX_HOME/sessions/2026/10/08"
rollout="$CODEX_HOME/sessions/2026/10/08/rollout-2026-10-08T10-00-00-019a1234-5678-7000-8000-000000000001.jsonl"
: > "$rollout"
if [ ! -e "$state/daemon.pid" ]; then
  nohup "$CC_HOME/real/codex" "$CC_CAPTURE/bin/daemon.sh" "$rollout" "$state" app-server >/dev/null 2>&1 &
  printf '{"pid":%s}' $! > "$state/daemon.pid"
fi
m=$'\033[35m'; b=$'\033[1m'; d=$'\033[2m'; r=$'\033[0m'
printf '\033[2J\033[H'
printf '%s╭────────────────────────────────────────────────────────────╮%s\n' "$m" "$r"
printf '%s│%s%s agent session%s%s · codex                                      %s%s│%s\n' "$m" "$r" "$b" "$r" "$d" "$r" "$m" "$r"
printf '%s╰────────────────────────────────────────────────────────────╯%s\n\n' "$m" "$r"
printf '> Add gift cards to the checkout\n\n'
printf '%s●%s Reading src/checkout.rs\n%s●%s Adding a GiftCard type with a balance\n' "$m" "$r" "$m" "$r"
printf '\n%s⠴ working…%s  %sesc to interrupt%s\n' "$m" "$r" "$d" "$r"
title='⠴ Add gift cards | web-shop'
[ "${CC_AGENT_STATE:-busy}" = waiting ] && title='[ ! ] Action Required | Add gift cards | web-shop'
[ "${CC_AGENT_STATE:-busy}" = idle ] && title='Add gift cards | web-shop'
sleep 1
sed "s|/tmp/project|$PWD|g" "$CC_DATA/codex-context.jsonl" > "$rollout"
printf '\033]0;%s\007' "$title"
while :; do sleep 1; done
