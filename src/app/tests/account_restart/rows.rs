use super::*;

fn rows(setup: &Setup) -> Vec<(u64, ui::AccountRow)> {
    let ids = setup.app.tab().expect("a tab").panes.iter().map(|term| term.id).collect::<Vec<_>>();
    ids.into_iter().zip(setup.app.account_rows()).collect()
}

fn row_of(setup: &Setup, pane: u64) -> ui::AccountRow {
    rows(setup).into_iter().find_map(|(id, row)| (id == pane).then_some(row)).expect("the pane's row")
}

fn pills(row: &ui::AccountRow) -> Vec<(&str, &str)> {
    row.pills.iter().map(|pill| (pill.agent.as_str(), pill.account.as_str())).collect()
}

impl Setup {
    pub(super) fn split_right(&mut self) -> u64 {
        let pane = self.app.layout(AREA).pane;
        right_click(&mut self.app, Position::new(pane.x + 2, pane.y + 2));
        pick(&mut self.app, "split right");
        self.app.resize(AREA);
        wait_until("the new shell is at its prompt", || self.app.term().is_some_and(Term::shell_in_foreground));
        self.app.term().expect("the new pane").id
    }

    pub(super) fn claude_on(&mut self, home: &Path) {
        let line = format!("env -u CLAUDE_CONFIG_DIR {} {}", self.claude.script().display(), home.display());
        type_line(&mut self.app, &line);
    }
}

#[test]
fn two_claudes_on_two_accounts_show_theirs_at_once() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let work = setup.start_on("work");
    setup.runs();
    let home = setup.add("personal");
    setup.app.relist_accounts();
    let personal = setup.split_right();
    setup.app.respawn_shell(personal, Known::Claude, "personal").expect("the shell starts on personal");
    wait_until("the shell is back", || setup.app.term().is_some_and(Term::shell_in_foreground));
    setup.claude_on(&home);
    setup.watch_until("both run claude", |app| {
        [work, personal].iter().all(|id| app.projects.pane_by(*id).is_some_and(|t| t.agent_account().is_some()))
    });

    assert_eq!(pills(&row_of(&setup, work)), [("claude", "work")]);
    assert_eq!(pills(&row_of(&setup, personal)), [("claude", "personal")]);
    let rows = setup.app.layout(AREA).pane;
    let row = rows.bottom() - 1;
    let buffer = render(&mut setup.app, AREA).backend().buffer().clone();
    let drawn: String = (rows.x..rows.right()).map(|x| buffer[(x, row)].symbol().to_string()).collect();
    assert!(drawn.contains("work ▾") && drawn.contains("personal ▾"), "{drawn}");
}

#[test]
fn a_shell_shows_a_button_per_agent_with_accounts_and_dims_them_behind_a_program() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    accounts::add(&setup.config.path().join("accounts"), Known::Codex, "work", None).expect("codex account");
    let pane = setup.shell_on("personal");
    setup.app.relist_accounts();

    let row = row_of(&setup, pane);
    assert_eq!((pills(&row), row.blocked), (vec![("claude", "personal"), ("codex", "System")], false));

    setup.runs_sleep(pane);
    let row = row_of(&setup, pane);
    assert_eq!((pills(&row), row.blocked), (vec![("claude", "personal"), ("codex", "System")], true));
}

#[test]
fn a_pane_running_claude_shows_only_claude_s_button() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    accounts::add(&setup.config.path().join("accounts"), Known::Codex, "work", None).expect("codex account");
    let pane = setup.start_on("work");
    setup.app.relist_accounts();
    setup.runs();

    let row = row_of(&setup, pane);
    assert_eq!((pills(&row), row.blocked), (vec![("claude", "work")], false));
}
