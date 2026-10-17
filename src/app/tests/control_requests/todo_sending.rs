use super::*;

fn todo_send(id: u64, pane: Option<u64>) -> Command {
    Command::TodoSend(wire::TodoSend { id, pane, ..wire::TodoSend::default() })
}

fn add(app: &mut App, text: &str) -> u64 {
    done(now(app, None, Command::Todo(wire::Todo::Add(text.into())))).todo.expect("an id")
}

fn is_done(app: &App, id: u64) -> bool {
    app.todos.item(id).expect("the item").done
}

mod sending_a_todo_to_an_agent {
    use super::*;

    #[test]
    fn records_the_prompt_answers_with_the_item_and_checks_it_off() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "fix the login");

        ask(&mut agent.app, None, todo_send(todo, Some(id)));
        let sent = done(agent.answered("the agent records the prompt"));

        assert_eq!((sent.todo, sent.ids.pane, is_done(&agent.app, todo)), (Some(todo), Some(id), true));
    }

    #[test]
    fn a_client_that_leaves_mid_send_still_gets_the_item_checked_once() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "fix the login");
        ask(&mut agent.app, None, todo_send(todo, Some(id)));
        agent.app.forget(CLIENT);

        let refused = error(now(&mut agent.app, None, todo_send(todo, Some(id))));
        agent.refreshing("the item is checked off", |a| is_done(a, todo));

        assert!(refused.contains("already being sent"), "{refused}");
        assert!(agent.app.toast.is_none());
    }

    #[test]
    fn an_item_edited_while_it_is_sent_stays_pending() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "fix the login");
        ask(&mut agent.app, None, todo_send(todo, Some(id)));
        agent.app.todos.edit(todo, "fix the logout");

        done(agent.answered("the agent records the prompt"));

        assert!(!is_done(&agent.app, todo));
    }

    #[test]
    fn an_item_whose_open_field_was_changed_stays_pending() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "fix the login");
        ask(&mut agent.app, None, todo_send(todo, Some(id)));
        let editor = todo::editor::Editor::new("fix the logout");
        agent.app.todo.field = Some(todo::Field { item: Some(todo), editor });

        done(agent.answered("the agent records the prompt"));

        assert!(!is_done(&agent.app, todo));
    }

    #[test]
    fn works_for_an_internal_client_through_the_app() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "write the docs");

        agent.app.send_todo(CLIENT, todo, id, Instant::now());
        let sent = done(agent.answered("the agent records the prompt"));

        assert_eq!((sent.todo, is_done(&agent.app, todo)), (Some(todo), true));
    }

    #[test]
    fn stays_pending_when_the_agent_shows_a_panel() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
        done(now(&mut agent.app, None, press_keys(id, &["p", "a", "n", "e", "l", "enter"])));
        agent.until("the panel shows", |a| pane_screen(a, id).contains("Shell details"));
        let todo = add(&mut agent.app, "fix the login");

        let refused = error(now(&mut agent.app, None, todo_send(todo, Some(id))));

        assert!(refused.contains("shows a dialog, a panel or its shell mode"), "{refused}");
        assert!(!is_done(&agent.app, todo));
    }

    #[test]
    fn stays_pending_when_the_pane_does_not_exist() {
        let (mut app, _rx) = app();
        let todo = add(&mut app, "fix the login");

        let refused = error(now(&mut app, None, todo_send(todo, Some(9999))));

        assert!(refused.contains("there is no pane 9999"), "{refused}");
        assert!(!is_done(&app, todo));
    }

    #[test]
    fn refuses_a_pane_that_runs_no_agent_and_stays_pending() {
        let (mut app, _rx) = app();
        let shell = first(&app);
        let todo = add(&mut app, "fix the login");

        let refused = error(now(&mut app, None, todo_send(todo, Some(shell))));

        assert!(refused.contains(&format!("pane {shell} runs no agent")), "{refused}");
        assert!(!is_done(&app, todo));
    }

    #[test]
    fn refuses_an_item_whose_send_is_in_flight() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        let todo = add(&mut agent.app, "fix the login");
        ask(&mut agent.app, None, todo_send(todo, Some(id)));

        let refused = error(now(&mut agent.app, None, todo_send(todo, Some(id))));

        assert!(refused.contains("already being sent"), "{refused}");
    }

    #[test]
    fn refuses_an_item_that_is_already_done() {
        let (mut app, _rx) = app();
        let todo = add(&mut app, "fix the login");
        done(now(&mut app, None, Command::Todo(wire::Todo::Done(todo))));

        let refused = error(now(&mut app, None, todo_send(todo, None)));

        assert!(refused.contains("already done"), "{refused}");
    }

    #[test]
    fn answers_not_found_for_an_unknown_item() {
        let (mut app, _rx) = app();

        let refused = error(now(&mut app, None, todo_send(404, None)));

        assert!(refused.contains("there is no todo 404"), "{refused}");
    }
}

mod the_todo_panel_send_button {
    use super::*;
    use crate::ui::todo as panel;

    fn panel_area(app: &App) -> Rect {
        app.layout(AREA).shown(app.nav).changes
    }

    fn open_and_press_send(app: &mut App, todo_index: usize) {
        if app.side != Some(Side::Todo) {
            click(app, app.layout(AREA).todo_button.as_position());
        }
        let area = panel_area(app);
        let row = panel::rows(area, &app.todo_view(area)).item(todo_index);
        click(app, panel::send(row).as_position());
    }

    fn pick(app: &mut App, i: usize) {
        let pos = menu_item_at(app, i);
        click(app, pos);
    }

    fn settle(agent: &mut Agent) {
        let (app, rx) = (&mut agent.app, &agent.rx);
        refreshing(app, rx, "the toast says how it went", |a| a.toast.is_some());
    }

    #[test]
    fn lists_the_agents_and_sends_the_text_to_the_chosen_one() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        agent.until("the agent is seen", |a| !a.agent_places().is_empty());
        let todo = add(&mut agent.app, "fix the login");

        open_and_press_send(&mut agent.app, 0);
        let labels = match &agent.app.overlay {
            Some(Overlay::Menu { actions, .. }) => agent.app.menu_labels(actions),
            _ => Vec::new(),
        };
        assert_eq!(labels.len(), 1, "{labels:?}");
        assert!(labels[0].ends_with("· claude"), "{labels:?}");

        pick(&mut agent.app, 0);
        settle(&mut agent);

        let toast = agent.app.toast.as_ref().expect("a toast");
        assert!(toast.message.starts_with("sent to claude in "), "{}", toast.message);
        assert_eq!(
            (toast.icon, is_done(&agent.app, todo), pane(&agent.app, id).agent.status().is_some()),
            (ui::ToastIcon::Check, true, true)
        );
    }

    #[test]
    fn with_no_agent_it_toasts_instead_of_opening_a_menu() {
        let (mut app, _rx) = app();
        add(&mut app, "fix the login");

        open_and_press_send(&mut app, 0);

        let toast = app.toast.as_ref().expect("a toast");
        assert_eq!((toast.message.as_str(), app.overlay.is_none()), ("no agent is running", true));
    }

    #[test]
    fn a_refusal_toasts_its_reason_and_leaves_the_item_pending() {
        let mut agent = Agent::new();
        let id = agent.start(None);
        agent.until("its input box is seen", |a| pane(a, id).input_seen.is_some());
        done(now(&mut agent.app, None, press_keys(id, &["p", "a", "n", "e", "l", "enter"])));
        agent.until("the panel shows", |a| pane_screen(a, id).contains("Shell details"));
        let todo = add(&mut agent.app, "fix the login");

        open_and_press_send(&mut agent.app, 0);
        pick(&mut agent.app, 0);

        let toast = agent.app.toast.as_ref().expect("a toast");
        assert!(toast.message.contains("shows a dialog, a panel or its shell mode"), "{}", toast.message);
        assert_eq!((toast.icon, is_done(&agent.app, todo)), (ui::ToastIcon::Bug, false));
    }
}
