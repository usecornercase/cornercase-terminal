#!/bin/sh
set -eu
CC_CAPTURE=$(cd "$(dirname "$0")" && pwd)
CC_ROOT=$(cd "$CC_CAPTURE/../.." && pwd)
CC_BIN="$CC_ROOT/target/debug/cornercase"
CC_TMUX=cc-capture
CC_TMP=$(cd "${TMPDIR:-/tmp}" && pwd -P)/cc-capture
export CC_CAPTURE CC_ROOT CC_BIN CC_TMUX CC_TMP
. "$CC_CAPTURE/lib.sh"
trap '[ -z "${CC_HOME:-}" ] || cc_stop' EXIT
cc_require
mkdir -p "$CC_TMP"
"$CC_ROOT/site/node_modules/.bin/esbuild" "$CC_ROOT/site/src/lib/demo/data.ts" --bundle --platform=node --format=esm --log-level=warning --outfile="$CC_TMP/data.mjs"
. "$CC_CAPTURE/screens.sh"
if [ $# -eq 0 ]; then
  set -- $CC_SCREENS
fi
for name in "$@"; do
  case " $CC_SCREENS " in
    *" $name "*) ;;
    *) echo "capture: no screen named $name"; exit 1 ;;
  esac
  cc_reset
  cc_home_fresh
  "screen_$(printf '%s' "$name" | tr - _)"
  cc_stop
done
