#!/bin/sh
CC_SCREENS="first-run sidebar-stacked groups hover-close reorder codex-context search sidebar-tree tab-bar sidebar-agents compact-bar compact-menu-workspaces compact-menu-projects compact-menu-agents picker picker-filter new-workspace new-workspace-plain pane-menu group-style rename-menu rename-form remove-workspace issues-list issues-token issues-people issues-person-picker issues-mine issue-detail issue-detail-end issue-agent-picker issue-started overview splits toast changes changes-commits changes-base changes-filter files files-file files-search files-names files-link todo todo-add settings-worktrees settings-agents settings-claude-modes settings-default-agent settings-issues settings-ui usage keys-menu remote-reconnecting"

pane_in() {
  cc status --json | node -e '
    const s = JSON.parse(require("fs").readFileSync(0, "utf8"));
    const [want, nth] = process.argv.slice(1);
    const panes = [];
    for (const p of s.projects) for (const w of p.workspaces) for (const t of w.tabs) for (const pane of t.panes) panes.push(pane);
    const hit = panes.filter((p) => String(p.path || "").endsWith(want));
    process.stdout.write(String(hit[Number(nth || 0)].id));
  ' "$1" "${2:-0}"
}

ws_in() {
  cc status --json | node -e '
    const s = JSON.parse(require("fs").readFileSync(0, "utf8"));
    const want = process.argv[1];
    for (const p of s.projects) for (const w of p.workspaces) if (String(w.path).endsWith(want)) process.stdout.write(String(w.id));
  ' "$1"
}

toasts_gone() {
  log="$CC_HOME/.local/state/cornercase/server.log"
  i=0
  while [ "$(grep -c 'activity: notify' "$log")" -lt "$1" ]; do
    i=$((i + 1))
    [ "$i" -lt 200 ] || { echo "capture: timed out waiting for $1 notifications"; exit 1; }
    sleep 0.1
  done
  node -e '
    const notes = require("fs").readFileSync(process.argv[1], "utf8").split("\n").filter((line) => line.includes("activity: notify"));
    setTimeout(() => {}, Math.max(0, Date.parse(notes[notes.length - 1].split(" ")[0]) + 2000 - Date.now()));
  ' "$log"
  i=0
  while cc_text | grep -qF 'needs you'; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || { echo "capture: the toast never went"; cc_text; exit 1; }
    sleep 0.1
  done
}

cc_s1_at() {
  cc_config "$CC_CONFIG"
  cc_session "$CC_CAPTURE/sessions/$CC_SESSION.json"
  cc_start "$1" "$2"
  cc_wait 'web-shop'
  sleep 1
  cc send --pane "$(pane_in code/web-shop 0)" --enter 'nvim src/returns/address.rs'
  cc send --pane "$(pane_in code/web-shop 1)" --enter 'git log --oneline --graph'
  cc send --pane "$(pane_in feat-dark-mode)" --enter "CC_AGENT_STATE=$CC_DARK_STATE CC_AGENT_TASK=dark CC_AGENT_PERCENT=$CC_DARK_PERCENT claude --permission-mode plan"
  cc send --pane "$(pane_in feat-gift-cards)" --enter 'codex'
  cc send --pane "$(pane_in code/orders-api)" --enter 'npm run dev'
  cc send --pane "$(pane_in fix-pagination)" --enter 'CC_AGENT_STATE=waiting CC_AGENT_TASK=pagination claude --permission-mode plan'
  cc send --pane "$(pane_in code/blog)" --enter "$CC_BLOG_COMMAND"
  cc send --pane "$(pane_in code/dotfiles)" --enter 'git status -sb'
  cc_wait_status 'nvim'
  cc_wait_status 'npm'
  cc_wait_status 'codex  working  gpt-5.4 · 20%'
  want=working
  notices=1
  [ "$CC_DARK_STATE" = waiting ] && want=waiting && notices=2
  cc_wait_status "claude  $want  Opus 5.5 · $CC_DARK_PERCENT%"
  cc_wait_status 'claude  waiting  Opus 5.5 · 7%'
  cc_wait_status '2 behind'
  toasts_gone "$notices"
  sleep 1
}

cc_s1() {
  cc_s1_at 110 38
}

screen_first_run() {
  cc_config '{}'
  git -C "$CC_HOME/code/web-shop" worktree remove "$CC_HOME/.cornercase/worktrees/web-shop/feat-gift-cards"
  cc_start 110 38 "$CC_HOME/code/web-shop"
  cc_wait 'feat/dark-mode'
  cc_wait '↓2'
  cc_shot first-run
}

screen_sidebar_stacked() {
  cc_s1
  cc_shot sidebar-stacked
}

screen_groups() {
  cc_s1
  cc_click '◆ personal'
  cc_hover 'Workspaces'
  cc_wait '▸ ◆ personal (2)             │'
  cc_shot groups
}

screen_hover_close() {
  cc_s1
  cc_hover 'feat/gift-cards'
  cc_wait '×'
  cc_shot hover-close
}

screen_reorder() {
  cc_s1
  cc_press '─────'
  cc_move '─────' 0 1
  cc_release '─────'
  cc_press 'dotfiles'
  cc_move 'blog' 0 0
  cc_move 'orders-api' 0 0
  cc_wait '─ │   7 '
  cc_wait 'dotfiles (1)'
  cc_shot reorder
  cc_keys Escape
}

screen_codex_context() {
  cc_s1
  cc_click 'feat/gift-cards'
  cc_hover 'Workspaces'
  cc_wait 'Add gift cards to the checkout'
  cc_wait '▌ ├ ◐ codex                    │'
  cc_shot codex-context
}

screen_search() {
  cc_s1
  cc_click 'search projects'
  cc_type feat
  cc_wait '⌕ feat'
  cc_wait 'feat/gift-cards'
  cc_shot search
}

screen_sidebar_tree() {
  CC_CONFIG='{"sidebar":"tree"}'
  CC_SESSION=tree
  cc_s1
  cc_shot sidebar-tree
}

agents_knobs() {
  CC_CONFIG='{"agents_section":true}'
  CC_SESSION=agents
  CC_DARK_PERCENT=23
  CC_DARK_STATE=waiting
  CC_BLOG_COMMAND='CC_AGENT_PERCENT=2 CC_AGENT_MODEL=claude-sonnet-5-5 claude'
}

screen_tab_bar() {
  CC_CONFIG='{"tabs":"top"}'
  cc_s1
  cc new-tab --workspace "$(ws_in feat-dark-mode)" >/dev/null
  cc_click 'feat/dark-mode'
  cc_hover 'Workspaces'
  cc_wait 'Opus 5.5 · 7%'
  cc_shot tab-bar
}

screen_sidebar_agents() {
  agents_knobs
  cc_s1_at 110 44
  cc_wait 'Sonnet 5.5'
  cc_wheel 'feat/dark-mode' down 1
  cc_wait '│   gpt-5.4 · 20%'
  cc_shot sidebar-agents
}

screen_compact_bar() {
  cc_s1_at 64 35
  cc_wait 'web-shop › main › nvim'
  cc_shot compact-bar
}

screen_compact_menu_workspaces() {
  cc_s1_at 64 35
  cc_click '≡'
  cc_wait '‹ Projects'
  cc_shot compact-menu-workspaces
}

screen_compact_menu_projects() {
  cc_s1_at 64 35
  cc_click '≡'
  cc_click '‹ Projects'
  cc_wait 'dotfiles (1)'
  cc_shot compact-menu-projects
}

screen_compact_menu_agents() {
  agents_knobs
  cc_s1_at 64 35
  cc_click '≡'
  cc_click '‹ Projects'
  cc_click 'Agents ›'
  cc_wait 'Sonnet 5.5'
  cc_shot compact-menu-agents
}

screen_picker() {
  cc_s1
  cc_click '+ New'
  cc_click 'open project'
  cc_wait 'mobile-app'
  cc_shot picker
}

screen_picker_filter() {
  cc_s1
  cc_click '+ New'
  cc_click 'open project'
  cc_wait 'mobile-app'
  cc_type ap
  cc_wait '› ~/code/ap'
  cc_wait 'enter goes into mobile-app'
  cc_shot picker-filter
}

screen_new_workspace() {
  cc_s1
  cc_click '+ New workspace'
  cc_wait 'with its own worktree'
  cc_type 'feat/login'
  cc_wait 'in ~/.cornercase/worktrees/web-shop/feat-login'
  cc_shot new-workspace
}

screen_new_workspace_plain() {
  cc_s1
  cc_click '+ New workspace'
  cc_wait 'with its own worktree'
  cc_click 'its own worktree'
  cc_wait '[ ] with its own worktree'
  cc_hover 'Workspaces'
  cc_type 'feat/login'
  cc_wait '› feat/login'
  cc_wait 'in ~/code/web-shop'
  cc_shot new-workspace-plain
}

screen_pane_menu() {
  cc_s1
  cc_rclick '~' 4
  cc_wait 'split right'
  cc_shot pane-menu
}

screen_group_style() {
  cc_s1
  cc_rclick '◆ personal'
  cc_click 'icon and colour'
  cc_wait '╭─ personal ─'
  cc_shot group-style
}

screen_rename_menu() {
  cc_s1
  cc_rclick 'orders-api'
  cc_wait 'rename project'
  cc_shot rename-menu
}

screen_rename_form() {
  cc_s1
  cc_rclick 'orders-api'
  cc_click 'rename project'
  cc_wait 'leave it empty to use the folder name'
  cc_shot rename-form
}

screen_remove_workspace() {
  cc_s1
  cc_hover 'feat/dark-mode'
  cc_wait '×'
  cc_click '×'
  cc_wait 'remove anyway'
  cc_shot remove-workspace
}

open_issues() {
  cc_s1
  cc_click 'Issues'
  cc_wait '#482'
}

open_people() {
  open_issues
  cc_click 'people'
  cc_wait 'GitHub assignee  anyone'
}

open_issue_482() {
  open_issues
  cc_click '#482'
  cc_wait 'opened by @ana'
}

screen_issues_list() {
  open_issues
  cc_shot issues-list
}

screen_issues_token() {
  open_issues
  cc_click 'Shortcut'
  cc_wait 'API token'
  cc_shot issues-token
}

screen_issues_people() {
  open_people
  cc_hover 'Issues · web-shop'
  cc_shot issues-people
}

screen_issues_person_picker() {
  open_people
  cc_click 'anyone'
  cc_wait '@marta'
  cc_shot issues-person-picker
}

screen_issues_mine() {
  open_people
  cc_click 'anyone'
  cc_wait '@marta'
  cc_click 'me      you'
  cc_wait 'GitHub assignee  me'
  cc_click 'done'
  cc_wait '#471'
  cc_hover 'Issues · web-shop'
  cc_shot issues-mine
}

screen_issue_detail() {
  open_issue_482
  cc_shot issue-detail
}

screen_issue_detail_end() {
  open_issue_482
  cc_wheel 'start' down 40
  cc_wait '@luis'
  cc_shot issue-detail-end
}

screen_issue_agent_picker() {
  open_issue_482
  cc_click 'agent…'
  cc_wait 'aider'
  cc_shot issue-agent-picker
}

screen_issue_started() {
  open_issue_482
  cc_click 'start   agent…'
  cc_wait 'aider'
  cc_click 'claude    claude'
  cc_wait '> https://github.com/acme/web-shop/issues/482' 40
  cc_wait_status 'claude  idle  Opus 5.5 · 7%'
  cc_wheel 'feat/dark-mode' down 3
  cc_hover 'Workspaces'
  cc_wait '▌ │   Opus 5.5 · 7%'
  cc_wait '  feat/dark-mode               │'
  cc_wait '    blog (1)                   │'
  cc_shot issue-started
}

issue_pane() {
  pane_in issue-482-returns-page-crashes-on-empty-address "${1:-0}"
}

issue_listed() {
  cc_wheel '+ New workspace' down 3
  cc_hover 'Workspaces'
  cc_wait "$1"
}

cc_s2() {
  CC_ISSUE_482=1
  export CC_ISSUE_482
  cc_home_fresh
  CC_SESSION=s2
  cc_s1_at "${1:-140}" "${2:-38}"
}

cc_s2_agent() {
  cc_s2 "$@"
  cc send --pane "$(issue_pane)" --enter 'CC_AGENT_STATE=busy CC_AGENT_TASK=482 claude --permission-mode plan'
  cc_wait 'issues/482'
}

cc_s2_shell() {
  cc_s2 "$@"
  cc send --pane "$(issue_pane)" --enter 'git status -sb'
  cc_wait '## issue-482'
  issue_listed '▌ ├ › bash                     │'
}

screen_overview() {
  cc_s2_agent 110 38
  cc split --pane "$(issue_pane)" --down -- cargo test >/dev/null
  cc_wait 'test result: FAILED'
  cc_wait_status 'claude  working  Opus 5.5 · 7%  ~/.cornercase/worktrees/web-shop/issue-482'
  issue_listed '▌ ├ ◐ claude           +1      │'
  cc_wait '▌ │   Opus 5.5 · 7%'
  cc_shot overview
}

splits_setup() {
  cc_s2 110 38
  cc split --pane "$(issue_pane)" -- git log --oneline --graph >/dev/null
  cc_wait 'Release 0.5.0'
  cc_press '│' 0 1 40
  cc_move '│' 48
  cc_release '│' 48
  cc split --pane "$(issue_pane 1)" --down -- git status -sb >/dev/null
  cc_wait '## issue-482'
  cc keys --pane "$(issue_pane)" ctrl+l
  cc send --pane "$(issue_pane)" --enter 'cargo test'
  cc_wait 'test result: FAILED'
  cc_wait '│ s ❯                                           │'
  issue_listed '▌ ├ › bash             +2      │'
}

screen_splits() {
  splits_setup
  cc_shot splits
}

screen_toast() {
  splits_setup
  cc_press 'running 7 tests'
  cc_move 'running 7 tests' 10 2
  cc_release 'running 7 tests' 10 2
  cc_wait 'copied'
  cc_shot toast
}

changes_open() {
  cc_s2_shell
  cc_click 'Changes' 0 -1
  cc_wait 'Uncommitted'
}

changes_commits() {
  changes_open
  cc_click 'Commits' 0 1 95
  cc_wait 'A src/returns/tests.rs'
}

screen_changes() {
  changes_open
  cc_hover 'first_line' 0 1 95
  cc_wait 'ask agent'
  cc_shot changes
}

screen_changes_commits() {
  changes_commits
  cc_hover 'Workspaces'
  cc_shot changes-commits
}

screen_changes_base() {
  changes_commits
  cc_click 'main' 0 1 95
  cc_wait 'Compare with'
  cc_shot changes-base
}

screen_changes_filter() {
  changes_open
  cc_click 'All' 0 1 95
  cc_wait '3 files'
  cc_click '⌕' 0 1 95
  cc_type '!tests.rs'
  cc_wait '2 of 3 files'
  cc_hover 'Workspaces'
  cc_shot changes-filter
}

files_open() {
  cc_s2_shell
  cc_click 'Files' 0 -1
  cc_wait 'text in the files'
  cc_click 'src' 0 1 95
  cc_wait 'returns'
  cc_click 'returns' 0 1 95
  cc_wait 'address.rs                       M'
}

screen_files() {
  files_open
  cc_hover 'Workspaces'
  cc_shot files
}

screen_files_file() {
  files_open
  cc_click 'address.rs' 0 1 95
  cc_wait 'first_line'
  cc_press 'first_line' 0 1 95
  cc_move 'lines.first()'
  cc_release 'lines.first()'
  cc_wait 'lines 11–12'
  cc_click '▎' 0 1 95
  cc_wait '-         self.lines.first().unw'
  cc_hover 'Workspaces'
  cc_shot files-file
}

screen_files_search() {
  files_open
  cc_click 'text in the files'
  cc_type first_line
  cc_wait '3 matches in 3 files'
  cc_hover 'Workspaces'
  cc_shot files-search
}

screen_files_names() {
  files_open
  cc_click '▤' 0 1 95
  cc_type retadd
  cc_wait '1 file'
  cc_hover 'Workspaces'
  cc_shot files-names
}

link_underlined() {
  i=0
  while ! tmux -L "$CC_TMUX" capture-pane -e -p -t 0 | grep -F "$1" | grep -qF "$(printf '\033[4m')"; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || { echo "capture: $1 is not underlined"; cc_text; exit 1; }
    sleep 0.1
  done
}

screen_files_link() {
  cc_s2
  cc new-tab --focus --workspace "$(ws_in issue-482-returns-page-crashes-on-empty-address)" -- grep -rn first_line src >/dev/null
  cc_wait 'address.rs:11'
  issue_listed '▌ ├ › bash                     │'
  cc_hover 'address.rs:11'
  link_underlined 'address.rs:11'
  cc_shot files-link
}

todo_items() {
  cc todo add 'fix the 500 on /returns when the address has no second line' >/dev/null
  cc todo add 'add a test for Address::first_line' >/dev/null
  cc todo add 'reply to the design review' >/dev/null
  cc todo add 'update the release notes' >/dev/null
}

screen_todo() {
  cc_s1_at 140 38
  todo_items
  dentist=$(cc todo add 'book the dentist' | tr -dc '0-9')
  domain=$(cc todo add 'renew the domain' | tr -dc '0-9')
  cc todo done "$dentist" >/dev/null
  cc todo done "$domain" >/dev/null
  cc_click 'TODO' 0 -1
  cc_wait 'clear done'
  cc_hover 'add a test for'
  cc_wait 'add a test for Address::first_line  ×'
  cc_shot todo
}

screen_todo_add() {
  cc_s1_at 140 38
  todo_items
  cc_click 'TODO' 0 -1
  cc_wait '+ New TODO'
  cc_click '+ New TODO'
  cc_type 'ask legal about the new return window'
  cc_wait 'window'
  cc_hover 'Workspaces'
  cc_shot todo-add
}

settings_open() {
  cc_s1
  cc_click 'Settings'
  cc_wait 'Worktrees'
}

settings_agents() {
  settings_open
  cc_click 'Agents'
  cc_wait 'default agent'
}

screen_settings_worktrees() {
  settings_open
  cc_shot settings-worktrees
}

screen_settings_agents() {
  settings_agents
  cc_hover '╭─ Settings'
  cc_shot settings-agents
}

screen_settings_default_agent() {
  settings_agents
  cc_click 'default agent'
  cc_wait 'Which agent takes an issue by default?'
  cc_hover '╭─ Settings'
  cc_shot settings-default-agent
}

screen_settings_claude_modes() {
  settings_agents
  cc_click 'claude                    default'
  cc_wait 'How should claude start?'
  cc_hover '╭─ Settings'
  cc_shot settings-claude-modes
}

screen_settings_issues() {
  settings_open
  cc_click 'Issues'
  cc_wait 'Jira site'
  cc_hover 'GitHub'
  cc_wait '↑  ↓  │'
  cc_shot settings-issues
}

screen_settings_ui() {
  settings_open
  cc_click 'UI'
  cc_wait 'counts'
  cc_hover '╭─ Settings'
  cc_shot settings-ui
}

screen_usage() {
  cc_s1
  cc_click 'Usage'
  cc_wait 'resets in' 15
  cc_wait 'max plan'
  cc_wait 'plus plan'
  cc_shot usage
}

screen_keys_menu() {
  CC_CONFIG='{"prefix_key":"ctrl+]"}'
  cc_s1
  cc_click 'bash'
  cc_wait 'Release 0.5.0'
  cc_hover 'Workspaces'
  cc_keys 'C-]'
  cc_wait 'next tab'
  cc_shot keys-menu
}

screen_remote_reconnecting() {
  CC_WINDOW="'$CC_BIN' remote devbox --command '$CC_BIN'"
  cc_s1
  kill "$(cat "$CC_DATA/ssh.pid")"
  cc_wait 'Connection refused' 30
  cc_wait 'connecting…'
  cc_shot remote-reconnecting
}
