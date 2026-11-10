use super::*;

const NOTICES: u64 = CLIENT;
const OTHERS: u64 = CLIENT + 1;

#[rstest::rstest]
#[case::with_desktop_notifications("auto", 1)]
#[case::without_them("off", 0)]
fn notify_is_told_to_the_readers_of_notices_and_only_them(#[case] desktop: &str, #[case] sent: usize) {
    let (mut app, _rx) = empty_app();
    app.config.desktop_notifications = desktop.into();
    app.clients.events.subscribe(NOTICES, Vec::new(), false, true);
    app.clients.events.subscribe(OTHERS, Vec::new(), false, false);

    done(now(&mut app, None, Command::Notify(wire::Notify { text: "the build\u{7} is ready".into() })));

    let events = app.take_events();
    assert_eq!(
        messages(&events, NOTICES),
        [("notify".to_string(), "the build is ready".to_string(), Vec::new(), wire::Ids::default())]
    );
    assert!(events.iter().all(|(client, _)| *client == NOTICES), "{events:?}");
    let notified = app.take_notifications().len();
    assert_eq!((toast(&app), notified), (Some("the build is ready"), sent));
}

#[test]
fn notify_with_no_text_tells_nobody() {
    let (mut app, _rx) = empty_app();
    app.clients.events.subscribe(NOTICES, Vec::new(), false, true);

    error(now(&mut app, None, Command::Notify(wire::Notify { text: " ".into() })));

    let events = app.take_events();
    assert!(events.is_empty(), "{events:?}");
}

#[cfg(unix)]
mod from_a_pane {
    use super::*;

    #[test]
    fn notify_names_the_pane_it_ran_in_and_reaches_that_panes_reader() {
        let (mut app, _rx) = app();
        let pane = term(&app, 0).id;
        let other = done(now(&mut app, None, new_tab(None))).ids.pane.expect("a pane");
        let ids = wire::Ids {
            project: Some(app.projects[0].id),
            workspace: Some(app.projects[0].workspaces[0].id),
            ..wire::Ids::default()
        };
        app.clients.events.subscribe(CLIENT, Vec::new(), false, true);
        app.clients.events.subscribe(CLIENT + 1, vec![pane], false, true);
        app.clients.events.subscribe(CLIENT + 2, vec![other], false, true);
        app.clients.events.subscribe(CLIENT + 3, vec![pane], false, false);
        let _ = app.take_events();

        done(now(&mut app, Some(pane), Command::Notify(wire::Notify { text: "from the pane".into() })));

        let events = app.take_events();
        let told = vec![("notify".to_string(), "from the pane".to_string(), vec![pane], ids)];
        assert_eq!(messages(&events, CLIENT), told);
        assert_eq!(messages(&events, CLIENT + 1), told);
        assert!(messages(&events, CLIENT + 2).is_empty(), "{events:?}");
        assert!(messages(&events, CLIENT + 3).is_empty(), "{events:?}");
    }
}
