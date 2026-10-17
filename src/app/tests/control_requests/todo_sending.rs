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
