use super::*;

fn agent(setup: &Setup, pane: u64) -> Option<(i32, SystemTime)> {
    setup.app.projects.pane_by(pane).and_then(|term| term.context.agent())
}

#[test]
fn choosing_an_account_in_a_pane_s_row_restarts_only_that_pane() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let first = setup.start_on("work");
    setup.runs();
    setup.add("personal");
    setup.app.relist_accounts();
    let second = setup.split_right();
    setup.claude_on(&setup.config.path().join("accounts/claude/work"));
    setup.watch_until("both run claude", |app| {
        [first, second].iter().all(|id| app.projects.pane_by(*id).is_some_and(|t| t.context.agent().is_some()))
    });
    let other = agent(&setup, second);
    setup.untouched(first);

    click_pill(&mut setup.app, first, 0);
    assert_eq!(menu_labels(&setup.app), ["  System", "  personal", "✓ work", "manage accounts…"]);
    pick(&mut setup.app, "  personal");
    setup.restarted(first);

    assert_eq!(toast(&setup.app), Some("restarting claude on personal"));
    let term = setup.app.projects.pane_by(first).expect("the same pane");
    assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
    assert_eq!(setup.app.projects.pane_by(second).map(Term::generation), Some(0));
    assert_eq!(agent(&setup, second), other);
    assert_eq!(setup.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
    setup.claude.signal("quit");
}

#[test]
fn a_working_agent_waits_and_its_row_cancels_or_points_the_restart_elsewhere() {
    let (mut setup, pane, _home) = waiting::working();
    setup.add("home");
    setup.app.relist_accounts();

    click_pill(&mut setup.app, pane, 0);
    pick(&mut setup.app, "  personal");
    assert_eq!(toast(&setup.app), Some("claude restarts on personal after this turn"));
    assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "personal")));

    click_pill(&mut setup.app, pane, 0);
    pick(&mut setup.app, "  home");
    assert_eq!(setup.app.restart_waiting(pane), Some((Known::Claude, "home")));

    click_pill(&mut setup.app, pane, 0);
    pick(&mut setup.app, "✓ work");
    assert_eq!(toast(&setup.app), Some("claude stays on work"));
    assert_eq!(setup.app.restart_waiting(pane), None);
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

#[test]
fn a_flow_member_s_accounts_are_disabled_and_say_why() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.start_on("work");
    setup.runs();
    setup.add("personal");
    setup.app.relist_accounts();
    setup.seat_coder(pane);

    click_pill(&mut setup.app, pane, 0);
    assert_eq!(
        menu_labels(&setup.app),
        ["  System", "  personal", "✓ work", "it is the coder of a flow run", "manage accounts…"]
    );
    assert_eq!(menu_disabled(&setup.app), [true, true, true, true, false]);
    let term = setup.app.projects.pane_by(pane).expect("the pane");
    assert!(!setup.app.pane_pills(term).blocked, "a flow's seat keeps its buttons lit");
    pick(&mut setup.app, "  personal");

    assert_eq!(setup.app.restart_waiting(pane), None);
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
    assert_eq!(toast(&setup.app), None);
    setup.claude.signal("quit");
}

#[test]
fn a_custom_account_keeps_its_button_lit_and_its_menu_disabled() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.start_on("work");
    setup.runs();
    setup.add("personal");
    setup.app.relist_accounts();
    setup.set_custom(pane);

    let term = setup.app.projects.pane_by(pane).expect("the pane");
    assert!(!setup.app.pane_pills(term).blocked);
    click_pill(&mut setup.app, pane, 0);
    let labels = menu_labels(&setup.app);
    assert_eq!(labels[labels.len() - 2], "its account was not set by cornercase");
    assert!(menu_disabled(&setup.app).iter().rev().skip(1).all(|disabled| *disabled));
    setup.claude.signal("quit");
}
