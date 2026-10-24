#!/bin/sh
CC_SCREENS="first-run tour sidebar-stacked workspace-folded groups hover-close reorder codex-context search search-sessions sidebar-tree tab-bar sidebar-agents close-agent close-agents agents-closed agent-failed compact-bar compact-menu-workspaces compact-menu-projects compact-menu-agents picker picker-filter new-workspace new-workspace-plain pane-menu open-in group-style rename-menu rename-form remove-workspace worktrees worktrees-confirm pull-requests pull-request-checks pull-request-stack issues-list issues-token issues-people issues-person-picker issues-mine issue-detail issue-detail-end issue-agent-picker issue-names issue-started overview splits toast changes changes-commits changes-base changes-filter changes-every changes-discard changes-stage files files-file files-full files-find files-search files-names files-link todo todo-add settings-worktrees settings-agents settings-flows settings-remote settings-claude-modes settings-gemini-modes settings-default-agent settings-issues settings-ui settings-share settings-import usage usage-stats usage-accounts settings-accounts account-pills tab-account keys-menu remote-reconnecting queue queue-form flow-run flow-agents flows-list flows-deleted flows-import flows-edit flow-start"

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
  log="$CC_RUN/server.log"
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

screen_tour() {
  cc_config '{}'
  rm -f "$CC_RUN/tour.json"
  cc_start 110 38 "$CC_HOME/code/web-shop"
  cc_wait 'Get started'
  cc_wait 'feat/dark-mode'
  cc_shot tour
}

screen_sidebar_stacked() {
  cc_s1
  cc_shot sidebar-stacked
}

screen_workspace_folded() {
  cc_s1
  cc_click '▾ feat/dark-mode'
  cc_wait '▸ feat/dark-mode'
  cc_shot workspace-folded
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

screen_search_sessions() {
  cc_s1
  cc_click 'search projects'
  cc_keys Tab
  cc_type checkout
  cc_wait '⌕ checkout'
  cc_wait 'enter goes to pane'
  cc_shot search-sessions
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

screen_close_agent() {
  agents_knobs
  cc_s1_at 110 44
  cc_wait 'Sonnet 5.5'
  cc_wheel 'feat/dark-mode' down 1
  cc_wait '│   gpt-5.4 · 20%'
  cc_hover 'web-shop › feat/dark'
  cc_wait '×'
  cc_click '×'
  cc_wait 'Close agent'
  cc_wait 'keep running'
  cc_shot close-agent
}

screen_close_agents() {
  cc_s1
  cc_rclick 'feat/dark-mode'
  cc_click 'close agents'
  cc_wait 'Close agents'
  cc_wait ' close agents '
  cc_shot close-agents
}

screen_agents_closed() {
  cc_s1
  cc_rclick 'feat/dark-mode'
  cc_click 'close agents'
  cc_wait ' close agents '
  cc_click ' close agents '
  cc_wait 'agent closed'
  cc_wait '▶'
  cc_shot agents-closed
}

screen_agent_failed() {
  agents_knobs
  CC_BLOG_COMMAND='CC_AGENT_STATE=failed CC_AGENT_PERCENT=2 CC_AGENT_MODEL=claude-sonnet-5-5 claude'
  cc_s1_at 110 44
  cc_wait_status 'claude  failed  Sonnet 5.5'
  cc_hover 'Workspaces'
  cc_shot agent-failed
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
  cc_s1_at 64 38
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

new_workspace_form() {
  cc_click '+ New' 0 -1
  cc_wait 'start a flow…'
  cc_click 'new workspace'
  cc_wait 'with its own worktree'
}

screen_new_workspace() {
  cc_s1
  new_workspace_form
  cc_type 'feat/login'
  cc_wait 'in ~/.cornercase/worktrees/web-shop/feat-login'
  cc_shot new-workspace
}

screen_new_workspace_plain() {
  cc_s1
  new_workspace_form
  cc_click 'its own worktree'
  cc_wait '[ ] with its own worktree'
  cc_hover 'Workspaces'
  cc_type 'feat/login'
  cc_wait '› feat/login'
  cc_wait 'in ~/code/web-shop'
  cc_shot new-workspace-plain
}

screen_pane_menu() {
  CC_LOCAL=1
  cc_s1
  cc_rclick '~' 4
  cc_wait 'open in…'
  cc_shot pane-menu
}

screen_group_style() {
  cc_s1
  cc_rclick '◆ personal'
  cc_click 'icon and colour'
  cc_wait '╭─ personal ─'
  cc_shot group-style
}

screen_open_in() {
  CC_LOCAL=1
  cc_s1
  cc_rclick 'orders-api'
  cc_click 'open in…'
  cc_wait 'file manager'
  cc_shot open-in
}

screen_rename_menu() {
  CC_LOCAL=1
  cc_s1
  cc_rclick 'orders-api'
  cc_wait 'open in…'
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

worktrees_open() {
  cc_s1
  cc_rclick 'web-shop'
  cc_click 'worktrees'
  cc_wait 'Worktrees › web-shop'
  cc_wait 'feat/gift-cards'
  cc_hover '╭─ Worktrees'
}

screen_worktrees() {
  worktrees_open
  cc_shot worktrees
}

screen_worktrees_confirm() {
  worktrees_open
  cc_click '○' 0 1
  cc_click '○' 0 1
  cc_wait ' remove 2 '
  cc_click ' remove 2 '
  cc_wait ' also delete the branch '
  cc_hover '╭─ Worktrees'
  cc_shot worktrees-confirm
}

pulls_shown() {
  touch "$CC_DATA/gh/pulls"
  cc_s1
  cc_wait '#485✗'
  cc_wait '#483•'
}

screen_pull_requests() {
  pulls_shown
  cc_shot pull-requests
}

screen_pull_request_checks() {
  pulls_shown
  cc_click '#485✗'
  cc_wait 'copy link'
  cc_shot pull-request-checks
}

screen_pull_request_stack() {
  touch "$CC_DATA/gh/stacked"
  pulls_shown
  cc_wait '2/2'
  cc_click '#483•'
  cc_wait 'stack #484'
  cc_shot pull-request-stack
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

screen_issue_names() {
  open_issue_482
  cc_click 'start   schedule   start a flow'
  cc_wait 'aider'
  cc_click 'claude    claude'
  cc_wait '› workspace #482'
  cc_shot issue-names
}

screen_issue_started() {
  open_issue_482
  cc_click 'start   schedule   start a flow'
  cc_wait 'aider'
  cc_click 'claude    claude'
  cc_wait '› workspace #482'
  cc_keys Enter
  cc_wait '> https://github.com/acme/web-shop/issues/482' 40
  cc_wait_status 'claude  idle  Opus 5.5 · 7%'
  cc_wheel 'feat/dark-mode' down 3
  cc_hover 'Workspaces'
  cc_wait '▌ │   Opus 5.5 · 7%'
  cc_wait '  ▾ feat/dark-mode             │'
  cc_wait '    blog (1)                   │'
  cc_shot issue-started
}

issue_pane() {
  pane_in issue-482-returns-page-crashes-on-empty-address "${1:-0}"
}

issue_listed() {
  cc_wheel '+ New' down 3 -1
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
  CC_CONFIG='{"icons":"on"}'
  cc_s2_shell
  cc_click 'Changes' 0 -1
  cc_wait 'Uncommitted'
}

changes_commits() {
  changes_open
  cc_click 'Commits' 0 1 95
  cc_wait 'src/returns/tests.rs'
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
  cc_click '' 0 1 95
  cc_type '!tests.rs'
  cc_wait '2 of 3 files'
  cc_hover 'Workspaces'
  cc_shot changes-filter
}

screen_changes_every() {
  changes_open
  cc_click 'Every project' 0 1 95
  cc_wait 'expand all'
  cc_hover 'Workspaces'
  cc_shot changes-every
}

screen_changes_discard() {
  changes_open
  cc_hover 'address.rs' 0 1 95
  cc_click '↺' 0 1 95
  cc_wait 'Discard changes'
  cc_shot changes-discard
}

screen_changes_stage() {
  changes_open
  cc_hover 'mod.rs' 0 1 95
  cc_click '+  ✓' 0 1 95
  i=0
  while ! cc changes list --json 2>/dev/null | grep -qE '"staged": ?"yes"'; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || { echo "capture: nothing was staged"; cc_text; exit 1; }
    sleep 0.2
  done
  cc_hover 'address.rs' 0 1 95
  cc_wait '↺'
  cc_shot changes-stage
}

files_open() {
  CC_CONFIG='{"icons":"on"}'
  cc_s2_shell
  cc_click 'Files' 0 -1
  cc_wait 'text in the files'
  cc_click 'src' 0 1 95
  cc_wait 'returns'
  cc_click 'returns' 0 1 95
  i=0
  while ! cc_text | grep -qE 'address\.rs +M'; do
    i=$((i + 1))
    [ "$i" -lt 200 ] || { echo "capture: address.rs never got its mark"; cc_text; exit 1; }
    sleep 0.1
  done
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
  cc_wait 'lines 11-12'
  cc_click '▎' 0 1 95
  cc_wait '-         self.lines.first().unw'
  cc_hover 'Workspaces'
  cc_shot files-file
}

screen_files_full() {
  files_open
  cc_click 'address.rs' 0 1 95
  cc_wait 'first_line'
  cc_press 'first_line' 0 1 95
  cc_move 'lines.first()'
  cc_release 'lines.first()'
  cc_wait 'lines 11-12'
  cc_click '' 0 1 95
  cc_wait ''
  cc_hover 'lines 11-12'
  cc_shot files-full
}

screen_files_find() {
  files_open
  cc_click 'address.rs' 0 1 95
  cc_wait 'first_line'
  cc_click '' 0 1 95
  cc_type line
  cc_wait '1/3'
  cc_keys Down
  cc_wait '2/3'
  cc_hover 'Workspaces'
  cc_shot files-find
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
  cc_click '' 0 1 95
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
  cc_wait '▸'
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
  cc_more settings
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

screen_settings_gemini_modes() {
  settings_agents
  cc_click 'gemini                    default'
  cc_wait 'How should gemini start?'
  cc_hover '╭─ Settings'
  cc_shot settings-gemini-modes
}

screen_settings_issues() {
  settings_open
  cc_click 'Issues'
  cc_wait 'Jira site'
  cc_hover '╭─ Settings'
  cc_wait 'IDs or links, empty lists your tasks'
  cc_shot settings-issues
}

screen_settings_remote() {
  settings_open
  cc_click 'Remote'
  cc_wait 'remote access'
  cc_hover '╭─ Settings'
  cc_shot settings-remote
}

screen_settings_ui() {
  settings_open
  cc_click 'UI'
  cc_wait 'counts'
  cc_hover '╭─ Settings'
  cc_shot settings-ui
}

share_open() {
  cc_config '{}'
  cc_session "$CC_CAPTURE/sessions/$CC_SESSION.json"
  cc_start 140 38
  cc_wait 'web-shop'
  cc_more settings
  cc_wait 'Worktrees'
  cc_click ' share ' 1
  cc_wait 'copy my setup'
}

screen_settings_share() {
  share_open
  cc_hover '╭─ Settings'
  cc_shot settings-share
}

screen_settings_import() {
  git -C "$CC_HOME/code/web-shop" remote set-url origin git@github.com:acme/web-shop.git
  share_open
  local file="$CC_TMP/team-setup.toml"
  cat > "$file" <<'TOML'
# A cornercase setup: import it with `cornercase config import <file>` or Settings → share
version = 1
connect = ["linear"]

[agents]
agent = "claude"
agent_args = { claude = ["--model", "opus"] }
prompt = "Work on {url}. Run the tests before you commit."
submit = true

[issues]
issue_branch = "{source}-{number}-{slug}"
issue_tabs = ["github", "linear"]

[worktrees]
fetch_minutes = 10

[interface]
sidebar = "tree"
counts = false

[[repositories]]
remotes = ["github.com/acme/web-shop"]
setup = "npm ci && cp ../.env.example .env"
teardown = ""
TOML
  tmux -L "$CC_TMUX" load-buffer "$file"
  tmux -L "$CC_TMUX" paste-buffer -p -t 0
  cc_wait 'version = 1'
  cc_click ' preview ' 1 -1
  cc_wait 'Settings › Import'
  cc_hover '╭─ Settings'
  cc_shot settings-import
}

screen_usage() {
  cc_s1
  cc_more usage
  cc_click 'Limits'
  cc_wait 'resets in' 15
  cc_wait 'max plan'
  cc_wait 'plus plan'
  cc_shot usage
}

accounts_knobs() {
  CC_HOMES=
}

accounts_add() {
  cc account add claude personal >/dev/null
  cc account rename claude --system work >/dev/null
  cc_wait ' claude: work ▾'
}

screen_usage_accounts() {
  accounts_knobs
  cc_s1_at 110 48
  accounts_add
  cc_more usage
  cc_click 'Limits'
  cc_wait '● Claude Code · work' 15
  cc_wait 'pro plan' 15
  cc_wait 'plus plan' 15
  cc_shot usage-accounts
}

screen_settings_accounts() {
  accounts_knobs
  cc_s1
  accounts_add
  cc_more settings
  cc_wait 'Worktrees'
  cc_click 'Accounts'
  cc_wait 'signed in as ana@acme.dev' 15
  cc_wait 'signed in as ana@example.com' 15
  cc_hover '╭─ Settings'
  cc_shot settings-accounts
}

screen_account_pills() {
  accounts_knobs
  cc_s1
  accounts_add
  cc_click 'Opus 5.5 · work'
  cc_wait 'esc to interrupt'
  cc_click ' claude: work ▾'
  cc_wait 'manage accounts'
  cc_hover '  personal'
  cc_shot account-pills
}

screen_tab_account() {
  accounts_knobs
  cc_s1
  accounts_add
  cc new-tab --workspace "$(ws_in feat-dark-mode)" --account claude=personal -- CC_AGENT_STATE=busy CC_AGENT_TASK=dark CC_AGENT_PERCENT=12 claude >/dev/null
  cc_click 'feat/dark-mode'
  cc_wait '· personal · 12%'
  cc_hover 'Workspaces'
  cc_shot tab-account
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


screen_usage_stats() {
  node --input-type=module <<'JS'
import fs from 'node:fs';
import path from 'node:path';
const now = Math.floor(Date.now() / 1000), hour = Math.floor(now / 3600);
const total = { by_agent: [{ agents: 212, prompts: 3118, turns: 2904, agent_ms: 1116000000, waited_ms: 100800000, tokens: { in: 90000000, out: 24000000, cache_read: 2400000000, cache_write: 0 } }, ...Array.from({ length: 3 }, () => ({ agents: 0, prompts: 0, turns: 0, agent_ms: 0, waited_ms: 0, tokens: { in: 0, out: 0, cache_read: 0, cache_write: 0 } }))], busy_ms: 300000000, most_at_once: 9 };
const hours = Array.from({ length: 182 }, (_, i) => [hour - i*24, [[i === 0 ? 4 : 4, i === 0 ? 38 : 61, i === 0 ? 35 : 57, i === 0 ? 22320000 : 20910000 + (i%4)*1000000, i === 0 ? 1320000 : 1980000, i === 0 ? 1200000 : 1630000, i === 0 ? 410000 : 448000, i === 0 ? 38000000 : 45000000, 0], ...Array.from({ length: 3 }, () => Array(9).fill(0))], i === 0 ? 6600000 : 6200000, i === 0 ? 5 : 7]);
const file = path.join(process.env.CC_RUN, 'stats.json');
fs.writeFileSync(file, JSON.stringify({ version: 1, since: now - 182 * 86400, total, hours }), { mode: 0o600 });
JS
  touch "$CC_DATA/gh/merged"
  git -C "$CC_HOME/code/web-shop" remote set-url origin git@github.com:acme/web-shop.git
  cc_s1
  cc_more usage
  cc_wait 'in parallel'
  cc_wait 'merged PRs'
  cc_shot usage-stats
}

flow_482() {
  cc_s1_at 140 38
  cc flow run 'Full issue' --prompt 'Fix https://github.com/acme/web-shop/issues/482: the returns page crashes on an empty address.' --worktree issue-482-returns-page-crashes-on-empty-address --focus >/dev/null
  cc_wait 'Sending issue #482 to coder' 60
}

screen_flow_run() {
  flow_482
  cc_wait 'findings · 1/3' 60
  cc_wait 'waiting for reviewer…' 30
  cc_wheel 'feat/dark-mode' down 3
  cc_hover 'Workspaces'
  cc_wait '├ ○ tester'
  cc_wait 'coder-2.md' 30
  i=0
  while cc_text | grep -qF 'finished in'; do
    i=$((i + 1))
    [ "$i" -lt 100 ] || { echo "capture: the toast never went"; cc_text; exit 1; }
    sleep 0.1
  done
  cc_shot flow-run
}

screen_flow_agents() {
  CC_CONFIG='{"agents_section":true}'
  cc_s1_at 110 44
  cc flow run 'Full issue' --prompt 'Fix https://github.com/acme/web-shop/issues/482: the returns page crashes on an empty address.' --worktree issue-482-returns-page-crashes-on-empty-address --focus >/dev/null
  cc_wait 'Sending issue #482 to coder' 60
  cc_wait 'tester' 30
  cc_find '─────' 2 -3
  tmux -L "$CC_TMUX" send-keys -t 0 -l "$(printf '\033[<0;%s;%sM\033[<32;%s;%sM\033[<0;%s;%sm' "$col" "$row" "$col" "$((row - 6))" "$col" "$((row - 6))")"
  sleep 0.3
  cc_wheel 'web-shop › feat/dark' down 1
  cc_hover 'Workspaces'
  cc_wait '└ ○ tester' 30
  cc_shot flow-agents
}

flows_open() {
  local config=${1-}
  [ -n "$config" ] || config='{}'
  cc_config "$config"
  cc_session "$CC_CAPTURE/sessions/$CC_SESSION.json"
  cc_start 140 38
  cc_wait 'web-shop'
  cc_more settings
  cc_click 'Flows   Issues'
  cc_wait 'a built-in flow: edit saves'
}

screen_settings_flows() {
  flows_open '{"default_flow":"Quick"}'
  cc_hover '╭─ Settings'
  cc_shot settings-flows
}

screen_flows_deleted() {
  cc_config '{"hidden_flows":["Arena","Cross review"]}'
  cc_session "$CC_CAPTURE/sessions/$CC_SESSION.json"
  cc_start 140 38
  cc_wait 'web-shop'
  cc_more settings
  cc_click 'Flows   Issues'
  cc_wait 'deleted built-ins'
  cc_click 'deleted built-ins'
  cc_wait 'enter hides them'
  cc_hover '╭─ Settings'
  cc_shot flows-deleted
}

flows_edit_full_issue() {
  flows_open
  cc_click 'Full issue'
  cc_click 'edit   duplicate'
  cc_wait 'not saved yet'
}

screen_flows_list() {
  flows_edit_full_issue
  cc_click 'save   done'
  cc_wait 'full-issue.toml'
  cc_click 'save   done' 7
  cc_wait 'open a pull request'
  cc_hover '╭─ Settings'
  cc_shot flows-list
}

screen_flows_import() {
  flows_open
  cc_click 'import'
  cc_wait 'paste a flow'
  local file="$CC_CAPTURE/quick-review.toml"
  cat > "$file" <<'TOML'
# A cornercase flow: import it with `cornercase flow import <file>` or Settings → Flows → import
name = "Quick review"
end = "commits"

[orchestrator]
agent = "claude"
model = "opus"
prompt = "Keep every message to an agent self-contained."

[[role]]
name = "coder"
agent = "codex"
edits = true
prompt = "Implement what the orchestrator asks, run the checks and commit locally."

[[role]]
name = "reviewer"
agent = "claude"
prompt = "Review the changes on this branch. Do not edit files."

[[stage]]
roles = ["coder"]

[[stage]]
roles = ["reviewer"]
TOML
  tmux -L "$CC_TMUX" load-buffer "$file"
  tmux -L "$CC_TMUX" paste-buffer -p -t 0
  cc_wait 'name = "Quick review"'
  cc_hover '╭─ Flows'
  cc_shot flows-import
}

screen_flows_edit() {
  flows_edit_full_issue
  cc_click '▸ 2'
  cc_wait 'goes back'
  cc_hover '╭─ Flows'
  cc_shot flows-edit
}

screen_flow_start() {
  CC_SESSION=s1
  cc_config '{}'
  cc_session "$CC_CAPTURE/sessions/$CC_SESSION.json"
  cc_start 140 38
  cc_wait 'web-shop'
  cc_click 'Issues'
  cc_wait '#482'
  cc_click '#482'
  cc_wait 'opened by @ana'
  cc_click 'start a flow'
  cc_wait 'Flow templates'
  cc_wait 'Full issue in web-shop'
  cc_wait '◆ open a pull request'
  cc_hover '╭─ Start a flow'
  cc_shot flow-start
}

screen_queue() {
  cc_config '{"sidebar":"projects_on_top"}'
  cc_session "$CC_CAPTURE/sessions/s1.json"
  node -e '
    const fs = require("fs");
    const path = process.argv[1];
    const project = process.argv[2] + "/code/web-shop";
    const labels = ["#482 Fix the login form", "Update the docs", "Fix logout", "Review the tests"];
    const states = [{state:"pending"}, {state:"started",pane:7,server:"previous-server"}, {state:"failed",reason:"codex is not installed"}, {state:"missed"}];
    const entries = labels.map((label, i) => ({id:i+1,at:Math.floor(Date.now()/3600000)*3600+3600+i*60,zone:"UTC",label,agent:"claude",command:"claude",program:"claude",mode:null,prompt:label,submit:true,place:{project,workspace:null,workspace_name:null,branch:"fix-login"},...states[i]}));
    fs.writeFileSync(path, JSON.stringify({version:1,next_id:4,entries}));
  ' "$CC_RUN/server.queue.json" "$CC_HOME"
  cc_start 140 38
  cc_wait 'web-shop'
  cc_click 'Issues'
  cc_wait '#482'
  cc_click 'Queue'
  cc_wait 'clear started'
  cc_hover 'Issues · web-shop'
  cc_shot queue
}

screen_queue_form() {
  cc_config '{"sidebar":"projects_on_top","agent":"claude"}'
  cc_session "$CC_CAPTURE/sessions/s1.json"
  cc_start 110 38
  cc_wait 'web-shop'
  cc_click 'Issues'
  cc_wait '#482'
  cc_click 'Queue'
  cc_wait 'nothing queued'
  cc_click 'new   cancel'
  cc_wait 'Schedule an agent'
  cc_type 'Fix the login form'
  cc_hover 'Workspaces'
  cc_shot queue-form
}
