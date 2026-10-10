#!/bin/sh
set -eu
mkdir -p "$CC_HOME/code" "$CC_HOME/.claude/sessions" "$CC_HOME/.codex/sessions" "$CC_HOME/.config/cornercase" "$CC_HOME/.local/state/cornercase" "$CC_HOME/.cornercase/worktrees" "$CC_HOME/real"
node "$CC_CAPTURE/dump.mjs" "$CC_TMP/data.mjs" "$CC_DATA"
cp "$CC_ROOT/tests/fixtures/codex/0.160.0/context.jsonl" "$CC_DATA/codex-context.jsonl"
for program in nvim npm cargo codex claude; do
  cp /bin/bash "$CC_HOME/real/$program"
  if command -v codesign >/dev/null 2>&1; then
    codesign -f -s - "$CC_HOME/real/$program" 2>/dev/null || { echo "capture: cannot sign $CC_HOME/real/$program"; exit 1; }
  fi
done

export GIT_AUTHOR_NAME=Ana GIT_AUTHOR_EMAIL=ana@example.com GIT_COMMITTER_NAME=Ana GIT_COMMITTER_EMAIL=ana@example.com
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

commit() {
  GIT_AUTHOR_DATE="$2" GIT_COMMITTER_DATE="$2" git -C "$1" commit -q --allow-empty -m "$3"
}

repo() {
  name=$1
  shift
  dir="$CC_HOME/code/$name"
  cp -R "$CC_DATA/trees/$name" "$dir"
  git -c init.defaultBranch=main init -q "$dir"
  git -C "$dir" add -A
  while [ $# -gt 0 ]; do
    commit "$dir" "$1" "$2"
    shift 2
  done
}

repo web-shop 2026-09-10T10:00:00Z 'Start the returns service' 2026-09-26T10:00:00Z 'Explain how to run it' 2026-10-05T10:00:00Z 'Release 0.5.0'
repo orders-api 2026-08-20T10:00:00Z 'Initial API' 2026-10-06T10:00:00Z 'Paginate orders'
repo blog 2026-08-01T10:00:00Z 'Set up Astro' 2026-10-03T10:00:00Z 'Publish the post on worktrees'
repo dotfiles 2025-10-01T10:00:00Z 'Initial dotfiles' 2026-09-08T10:00:00Z 'Move to nvim'
repo mobile-app 2026-09-17T10:00:00Z 'Initial app'
repo payments 2026-08-12T10:00:00Z 'Start the payments service' 2026-10-05T10:00:00Z 'Retry failed webhooks'
cp -R "$CC_DATA/trees/playground" "$CC_HOME/code/playground"

origin="$CC_HOME/code/.origins/web-shop.git"
mkdir -p "$CC_HOME/code/.origins"
git clone -q --bare "$CC_HOME/code/web-shop" "$origin"
git -C "$CC_HOME/code/web-shop" remote add origin "$origin"
git -C "$CC_HOME/code/web-shop" fetch -q origin
git -C "$CC_HOME/code/web-shop" branch -q -u origin/main main
git -C "$CC_HOME/code/web-shop" remote set-head origin main
ahead="$CC_TMP/ahead"
rm -rf "$ahead"
git clone -q "$origin" "$ahead"
commit "$ahead" 2026-10-07T09:00:00Z 'Fix the refund email currency'
commit "$ahead" 2026-10-07T16:00:00Z 'Bump axum to 0.8.4'
git -C "$ahead" push -q origin main
git -C "$CC_HOME/code/web-shop" fetch -q origin

wt="$CC_HOME/.cornercase/worktrees"
mkdir -p "$wt/web-shop" "$wt/orders-api"
git -C "$CC_HOME/code/web-shop" worktree add -q -b feat/dark-mode "$wt/web-shop/feat-dark-mode"
git -C "$CC_HOME/code/web-shop" worktree add -q -b feat/gift-cards "$wt/web-shop/feat-gift-cards"
git -C "$CC_HOME/code/orders-api" worktree add -q -b fix/pagination "$wt/orders-api/fix-pagination"
printf '\npub fn background() -> &%sstatic str {\n    "#121018"\n}\n' "'" >> "$wt/web-shop/feat-dark-mode/src/theme.rs"

if [ "${CC_ISSUE_482:-0}" = 1 ]; then
  issue="$wt/web-shop/issue-482-returns-page-crashes-on-empty-address"
  git -C "$CC_HOME/code/web-shop" worktree add -q -b issue-482-returns-page-crashes-on-empty-address "$issue"
  cp "$CC_DATA/fixed/tests.rs" "$issue/src/returns/tests.rs"
  git -C "$issue" add -A
  commit "$issue" 2026-10-08T11:00:00Z 'Add a test for empty addresses'
  cp "$CC_DATA/fixed/address.rs" "$issue/src/returns/address.rs"
  cp "$CC_DATA/fixed/mod.rs" "$issue/src/returns/mod.rs"
fi
