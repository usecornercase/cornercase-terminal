use super::*;

#[test]
fn send_pastes_and_keys_press_keys() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "echo cat-\"\"starts; cat -v");
    pump_until(&mut app, &rx, "cat runs", |a| pane_screen(a, id).contains("cat-starts"));

    done(now(&mut app, None, send_text(id, "hello", false, false)));
    done(now(&mut app, None, press_keys(id, &["ctrl+b", "enter"])));

    pump_until(&mut app, &rx, "cat echoes both", |a| pane_screen(a, id).contains("hello^B"));
}

#[test]
fn read_gives_the_screen_or_its_last_lines() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "printf 'one\\ntwo\\nthree-%s\\n' $((1+1))");
    pump_until(&mut app, &rx, "the lines show", |a| pane_screen(a, id).contains("three-2"));

    let read = |lines| Command::Read(wire::Read { pane: Some(id), tab: None, lines });
    let screen = done(now(&mut app, None, read(None))).text.expect("the screen");
    let last = done(now(&mut app, None, read(Some(3)))).text.expect("the lines");

    assert!(screen.contains("one\ntwo\nthree-2"), "{screen}");
    assert_eq!((last.lines().count(), last.contains("three-2")), (3, true), "{last}");
}

#[test]
fn the_last_message_needs_an_agent_that_keeps_one() {
    let (mut app, _rx) = app();
    let id = first(&app);
    let last = Command::LastMessage(wire::LastMessage { pane: Some(id), tab: None });

    let message = error(now(&mut app, None, last));

    assert!(message.starts_with(&format!("pane {id} runs no agent, and cornercase only reads")), "{message}");
}

#[test]
fn wait_until_shell_ends_once_the_program_does() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "sleep 1");
    pump_until(&mut app, &rx, "sleep runs", |a| !pane(a, id).shell_in_foreground());

    ask(&mut app, None, wait_for(id, Until::Shell, None));
    app.refresh(Instant::now());
    let early = answers(&mut app);

    let ended = done(answered(&mut app, &rx, "sleep ends"));
    assert_eq!((early, ended.ended.as_deref()), (Vec::new(), Some("shell")));
}

#[test]
fn wait_until_shell_right_after_typing_waits_for_the_program() {
    let (mut app, rx) = app();
    ask(&mut app, None, new_tab(Some("sleep 1")));
    let typed = wait_after_typing(&mut app, &rx);
    done(answered(&mut app, &rx, "sleep ends"));

    assert!(typed.elapsed() >= Duration::from_millis(900), "it ended after {:?}", typed.elapsed());
}

#[test]
fn a_shell_busy_before_its_program_starts_is_not_back_yet() {
    let (mut app, rx) = app();
    ask(&mut app, None, new_tab(Some("x=$(sleep 0.6); sleep 1")));
    let typed = wait_after_typing(&mut app, &rx);
    done(answered(&mut app, &rx, "the program ends"));

    assert!(typed.elapsed() >= Duration::from_millis(1500), "it ended after {:?}", typed.elapsed());
}

#[cfg(target_os = "linux")]
#[rstest::rstest]
#[case::send(|pane| send_text(pane, "", true, false), "did not take what was typed")]
#[case::keys(|pane| press_keys(pane, &["enter"]), "is not reading its input")]
fn an_enter_a_full_pane_refuses_is_not_reported_as_pressed(#[case] enter: fn(u64) -> Command, #[case] refused: &str) {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "stty -echo; sleep 30");
    pump_until(&mut app, &rx, "sleep runs", |a| pane(a, id).program(&a.config).as_deref() == Some("sleep"));
    let full = vec![b'x'; crate::term::MAX_QUEUED + 1024 * 1024];
    let filled = app.projects[0].workspaces[0].tabs[0].panes[0].write(&full);

    ask(&mut app, None, enter(id));

    let message = error(answered(&mut app, &rx, "the enter is refused"));
    assert!(filled && message.contains(refused), "{message}");
    assert_eq!(pane(&app, id).submitted, None, "the pane counts no Enter");
}

#[test]
fn wait_for_text_takes_a_line_already_on_the_screen() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "echo ready-$((1+1))");
    pump_until(&mut app, &rx, "the line shows", |a| pane_screen(a, id).contains("ready-2"));

    ask(&mut app, None, wait_for(id, Until::Text(r"ready-\d".into()), None));

    let ended = done(answered(&mut app, &rx, "the line matches"));
    let line = ended.line.unwrap_or_default();
    assert_eq!((ended.ended.as_deref(), line.ends_with("ready-2")), (Some("text"), true), "{line}");
}

#[test]
fn a_wait_times_out_saying_what_it_waited_for() {
    let (mut app, rx) = app();
    let id = first(&app);

    ask(&mut app, None, wait_for(id, Until::Text("never".into()), Some(0.05)));

    let message = error(answered(&mut app, &rx, "the wait times out"));
    assert_eq!(message, format!("timed out after 0.05s: no line on the screen of pane {id} matches `never`"));
}

#[test]
fn a_timeout_past_the_clock_is_refused_and_a_long_quiet_is_kept() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, None, wait_for(id, Until::Quiet(1e19), Some(1e19))));
    ask(&mut app, None, wait_for(id, Until::Quiet(1e19), None));
    let tick = app.tick(Instant::now());

    assert_eq!((message.as_str(), answers(&mut app), tick), ("--timeout is too long", Vec::new(), None));
}

#[test]
fn waiting_for_an_agent_needs_one() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, None, wait_for(id, Until::Stops, None)));

    assert!(message.contains(&format!("pane {id} runs no agent")), "{message}");
}

#[test]
fn waiting_for_an_agent_cornercase_cannot_follow_says_so() {
    let (mut app, rx) = app();
    let bin = TempDir::new();
    let aider = bin.path().join("aider");
    write_executable(&aider, "#!/bin/sh\nsleep 30\n");
    let id = first(&app);
    type_line(&mut app, &aider.display().to_string());
    pump_until(&mut app, &rx, "aider runs", |a| {
        crate::agents::detect(&a.config, &pane(a, id).foreground_args()).is_some()
    });

    let message = error(now(&mut app, None, wait_for(id, Until::Stops, None)));

    assert!(message.contains(&format!("pane {id} runs aider")), "{message}");
}

#[test]
fn a_pane_does_not_wait_for_its_own_shell() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, Some(id), wait_for(id, Until::Shell, None)));

    assert!(message.contains("it would wait for itself"), "{message}");
}

#[rstest::rstest]
#[case::one_pane(|id| wait_for(id, Until::Text("gone-\\d".into()), None))]
#[case::several(|id| wait_on(&[id], &[], true, Until::Text("gone-\\d".into()), None))]
fn a_client_that_leaves_takes_its_wait_with_it(#[case] wait: fn(u64) -> Command) {
    let (mut app, rx) = app();
    let id = first(&app);
    ask(&mut app, None, wait(id));

    app.forget(CLIENT);
    type_line(&mut app, "echo gone-$((1+1))");
    pump_until(&mut app, &rx, "the line shows", |a| pane_screen(a, id).contains("gone-2"));
    app.refresh(Instant::now());

    assert_eq!(answers(&mut app), []);
}

fn new_shell(app: &mut App, rx: &Receiver<AppEvent>) -> Done {
    let made = done(now(app, None, new_tab(None)));
    let pane = made.ids.pane.expect("the new pane");
    pump_until(app, rx, "the new shell is ready", |a| pane_screen(a, pane).contains('$'));
    made
}

fn typed_in(app: &mut App, tab: usize, line: &str) {
    let term = &mut app.projects[0].workspaces[0].tabs[tab].panes[0];
    assert!(term.write(format!("{line}\r").as_bytes()), "the shell takes the line");
}

fn ended(tab: Option<u64>, pane: Option<u64>, ended: &str, line: Option<&str>) -> Done {
    Done {
        ids: wire::Ids { tab, pane, ..wire::Ids::default() },
        ended: Some(ended.into()),
        line: line.map(str::to_string),
        ..Done::default()
    }
}

#[test]
fn waiting_for_any_of_several_answers_the_first_with_the_id_it_was_given() {
    let (mut app, rx) = app();
    let id = first(&app);
    let made = new_shell(&mut app, &rx);
    let tab = made.ids.tab.expect("the new tab");
    ask(&mut app, None, wait_on(&[id], &[tab], false, Until::Text(r"mark-\d".into()), None));
    app.refresh(Instant::now());
    let early = answers(&mut app);

    typed_in(&mut app, 1, "echo mark-$((1+1))");

    let answer = done(answered(&mut app, &rx, "the line shows in the tab"));
    assert_eq!(early, []);
    assert_eq!(answer.panes, [ended(Some(tab), made.ids.pane, "text", Some("mark-2"))]);
}

#[test]
fn waiting_for_all_answers_once_every_pane_has_matched_in_the_order_given() {
    let (mut app, rx) = app();
    let id = first(&app);
    let other = new_shell(&mut app, &rx).ids.pane.expect("the new pane");
    ask(&mut app, None, wait_on(&[id, other], &[], true, Until::Text(r"mark-\d".into()), None));

    typed_in(&mut app, 1, "echo mark-$((1+1))");
    pump_until(&mut app, &rx, "the second pane prints", |a| pane_screen(a, other).contains("mark-2"));
    app.refresh(Instant::now());
    let early = answers(&mut app);
    typed_in(&mut app, 0, "echo mark-$((2+1))");

    let answer = done(answered(&mut app, &rx, "the first pane prints too"));
    assert_eq!(early, []);
    assert_eq!(
        answer.panes,
        [ended(None, Some(id), "text", Some("mark-3")), ended(None, Some(other), "text", Some("mark-2"))]
    );
}

#[test]
fn a_pane_that_closes_ends_a_wait_on_several_as_closed() {
    let (mut app, rx) = app();
    let id = first(&app);
    let made = new_shell(&mut app, &rx);
    let other = made.ids.pane.expect("the new pane");
    ask(&mut app, None, wait_on(&[other, id], &[], true, Until::Text(r"mark-\d".into()), None));

    let close = wire::Close {
        item: Item::Tab(made.ids.tab.expect("the tab")),
        remove_worktree: false,
        force: false,
        unlock: false,
    };
    done(now(&mut app, None, Command::Close(close)));
    pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
    app.refresh(Instant::now());
    let early = answers(&mut app);
    typed_in(&mut app, 0, "printf '\\nmark-%s\\n' $((1+1))");

    let answer = done(answered(&mut app, &rx, "the open pane prints"));
    assert_eq!(early, []);
    assert_eq!(answer.panes, [ended(None, Some(other), "closed", None), ended(None, Some(id), "text", Some("mark-2"))]);
}

#[test]
fn a_wait_on_several_refuses_a_pane_that_is_not_there_and_times_out_naming_the_rest() {
    let (mut app, rx) = app();
    let id = first(&app);
    let other = new_shell(&mut app, &rx).ids.pane.expect("the new pane");

    let refused = error(now(&mut app, None, wait_on(&[id, 999_999], &[], false, Until::Shell, None)));
    ask(&mut app, None, wait_on(&[id, other], &[], false, Until::Text("never".into()), Some(0.05)));

    let message = error(answered(&mut app, &rx, "the wait times out"));
    assert_eq!(refused, "there is no pane 999999; `cornercase status` lists them");
    assert_eq!(
        message,
        format!(
            "timed out after 0.05s: no line on the screen of pane {id} matches `never`; \
                         no line on the screen of pane {other} matches `never`"
        )
    );
}
