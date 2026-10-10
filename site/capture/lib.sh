#!/bin/sh

cc_require() {
  for tool in tmux git node; do
    command -v "$tool" >/dev/null 2>&1 || { echo "capture needs $tool on the PATH"; exit 1; }
  done
  [ -x "$CC_ROOT/site/node_modules/.bin/esbuild" ] || { echo "capture needs site/node_modules: run npm ci in site/"; exit 1; }
  [ -x "$CC_ROOT/target/debug/cornercase" ] || { echo "capture needs target/debug/cornercase: run cargo build"; exit 1; }
}

cc_reset() {
  CC_CONFIG='{}'
  CC_SESSION=s1
  CC_DARK_STATE=busy
  CC_DARK_PERCENT=7
  CC_BLOG_COMMAND='git log --oneline'
  CC_ISSUE_482=0
  CC_WINDOW="'$CC_BIN'"
  export CC_ISSUE_482
}

cc_env() {
  env -i \
    HOME="$CC_HOME" \
    PATH="$CC_CAPTURE/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    TERM=xterm-256color \
    LANG=en_US.UTF-8 \
    SHELL=/bin/bash \
    PS1='\W ❯ ' \
    BASH_SILENCE_DEPRECATION_WARNING=1 \
    XDG_CONFIG_HOME="$CC_HOME/.config" \
    XDG_STATE_HOME="$CC_HOME/.local/state" \
    XDG_RUNTIME_DIR="$CC_RUN" \
    CLAUDE_CONFIG_DIR="$CC_HOME/.claude" \
    CODEX_HOME="$CC_HOME/.codex" \
    CC_CAPTURE="$CC_CAPTURE" \
    CC_HOME="$CC_HOME" \
    CC_DATA="$CC_DATA" \
    "$@"
}

cc() {
  cc_env "$CC_BIN" "$@"
}

cc_home_fresh() {
  CC_HOME="$CC_TMP/home"
  CC_RUN="$CC_TMP/run"
  CC_DATA="$CC_HOME/.capture"
  export CC_HOME CC_RUN CC_DATA
  cc kill-server >/dev/null 2>&1 || true
  tmux -L "$CC_TMUX" kill-server 2>/dev/null || true
  rm -rf "$CC_HOME" "$CC_RUN"
  mkdir -p "$CC_RUN"
  chmod 700 "$CC_RUN"
  sh "$CC_CAPTURE/home.sh"
}

cc_session() {
  mkdir -p "$CC_HOME/.local/state/cornercase"
  sed "s|__HOME__|$CC_HOME|g" "$1" > "$CC_HOME/.local/state/cornercase/session.json"
}

cc_config() {
  mkdir -p "$CC_HOME/.config/cornercase"
  printf '%s\n' "$1" > "$CC_HOME/.config/cornercase/config.json"
}

cc_start() {
  CC_ROWS=$2
  export CC_ROWS
  tmux -L "$CC_TMUX" kill-server 2>/dev/null || true
  tmux -L "$CC_TMUX" -f /dev/null new-session -d -x "$1" -y "$2" -c "${3:-$CC_HOME}" \
    "env -i HOME='$CC_HOME' PATH='$CC_CAPTURE/bin:/usr/bin:/bin:/usr/sbin:/sbin' TERM=xterm-256color LANG=en_US.UTF-8 SHELL=/bin/bash PS1='\\W ❯ ' BASH_SILENCE_DEPRECATION_WARNING=1 XDG_CONFIG_HOME='$CC_HOME/.config' XDG_STATE_HOME='$CC_HOME/.local/state' XDG_RUNTIME_DIR='$CC_RUN' CLAUDE_CONFIG_DIR='$CC_HOME/.claude' CODEX_HOME='$CC_HOME/.codex' CC_CAPTURE='$CC_CAPTURE' CC_HOME='$CC_HOME' CC_DATA='$CC_DATA' $CC_WINDOW"
  tmux -L "$CC_TMUX" set-option -g status off
  tmux -L "$CC_TMUX" resize-window -x "$1" -y "$2"
}

cc_text() {
  tmux -L "$CC_TMUX" capture-pane -p -t 0
}

cc_wait() {
  i=0
  while ! cc_text | grep -qF -- "$1"; do
    i=$((i + 1))
    [ "$i" -lt "$(( ${2:-20} * 10 ))" ] || { echo "capture: timed out waiting for '$1'"; cc_text; exit 1; }
    sleep 0.1
  done
}

cc_wait_status() {
  i=0
  while ! cc status 2>/dev/null | grep -qF -- "$1"; do
    i=$((i + 1))
    [ "$i" -lt "$(( ${2:-20} * 5 ))" ] || { echo "capture: timed out waiting for '$1' in status"; cc status; exit 1; }
    sleep 0.2
  done
}

cc_find() {
  hit=$(cc_text | node -e '
    const [text, dx, nth, after] = process.argv.slice(1);
    const hits = [];
    require("fs").readFileSync(0, "utf8").split("\n").forEach((line, i) => {
      const j = line.indexOf(text, Number(after));
      if (j >= 0) hits.push(`${i + 1} ${j + 1 + Number(dx)}`);
    });
    const k = Number(nth) < 0 ? hits.length + Number(nth) : Number(nth) - 1;
    if (k < 0 || k >= hits.length) process.exit(1);
    process.stdout.write(hits[k]);
  ' "$1" "${2:-0}" "${3:-1}" "${4:-0}") || hit=
  [ -n "$hit" ] || { echo "capture: '$1' is not on screen"; cc_text; exit 1; }
  row=${hit% *}
  col=${hit#* }
}

cc_at() {
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<%s;%s;%sM\033[<%s;%s;%sm' "${3:-0}" "$1" "$2" "${3:-0}" "$1" "$2")"
  sleep 0.3
}

cc_click() {
  cc_find "$1" "${2:-0}" "${3:-1}" "${4:-0}"
  cc_at "$col" "$row" 0
}

cc_rclick() {
  cc_find "$1" "${2:-0}" "${3:-1}" "${4:-0}"
  cc_at "$col" "$row" 2
}

cc_hover() {
  cc_find "$1" "${2:-0}" "${3:-1}" "${4:-0}"
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<35;%s;%sM' "$col" "$row")"
  sleep 0.3
}

cc_press() {
  cc_find "$1" "${2:-0}" "${3:-1}" "${4:-0}"
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<0;%s;%sM' "$col" "$row")"
  sleep 0.2
}

cc_move() {
  cc_find "$1" "${2:-0}"
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<32;%s;%sM' "$col" "$((row + ${3:-0}))")"
  sleep 0.2
}

cc_release() {
  cc_find "$1" "${2:-0}"
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<0;%s;%sm' "$col" "$((row + ${3:-0}))")"
  sleep 0.3
}

cc_wheel() {
  cc_find "$1"
  button=64
  [ "$2" = down ] && button=65
  n=${3:-1}
  while [ "$n" -gt 0 ]; do
    tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<%s;%s;%sM' "$button" "$col" "$row")"
    n=$((n - 1))
    sleep 0.1
  done
}

cc_keys() {
  tmux -L "$CC_TMUX" send-keys -t 0 "$@"
  sleep 0.3
}

cc_type() {
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$1"
  sleep 0.4
}

cc_shot() {
  sleep 0.6
  out="$CC_ROOT/site/src/screens/$1.ansi"
  width=$(tmux -L "$CC_TMUX" display-message -p -t 0 '#{pane_width}')
  tmux -L "$CC_TMUX" capture-pane -e -p -N -t 0 | node -e '
    const width = Number(process.argv[1]);
    let fg = "39", bg = "49", ul = "", attrs = [];
    const lines = require("fs").readFileSync(0, "utf8").replace(/\n$/, "").split("\n");
    process.stdout.write(lines.map((line) => {
      for (const [, raw] of line.matchAll(/\x1b\[([0-9;:]*)m/g)) {
        const params = raw === "" ? ["0"] : raw.split(";");
        for (let i = 0; i < params.length; i++) {
          const p = params[i];
          if (p === "38" || p === "48" || p === "58") {
            const n = params[i + 1] === "5" ? 2 : 4;
            const value = params.slice(i, i + n + 1).join(";");
            i += n;
            if (p === "38") fg = value;
            else if (p === "48") bg = value;
            else ul = value;
          } else if (p === "0" || p === "") [fg, bg, ul, attrs] = ["39", "49", "", []];
          else if (/^(3[0-79]|9[0-7])$/.test(p)) fg = p;
          else if (/^(4[0-79]|10[0-7])$/.test(p)) bg = p;
          else if (p === "59") ul = "";
          else attrs.push(p);
        }
      }
      const pad = " ".repeat(Math.max(0, width - [...line.replace(/\x1b\[[0-9;:]*m/g, "")].length));
      const pen = [...attrs, fg, bg, ul].filter(Boolean).join(";");
      return !pad || pen === "39;49" ? line + pad : `${line}\x1b[0m${pad}\x1b[0;${pen}m`;
    }).join("\n") + "\n");
  ' "$width" > "$CC_TMP/shot.ansi"
  lines=$(wc -l < "$CC_TMP/shot.ansi" | tr -d ' ')
  [ "$lines" -eq "$CC_ROWS" ] || { echo "capture: $1 has $lines rows, wanted $CC_ROWS"; exit 1; }
  mv "$CC_TMP/shot.ansi" "$out"
  echo "captured $1"
}

cc_stop() {
  cc kill-server >/dev/null 2>&1 || true
  rm -rf "$CC_HOME/.codex/app-server-daemon" "$CC_DATA/ssh.calls"
  tmux -L "$CC_TMUX" kill-server 2>/dev/null || true
}
