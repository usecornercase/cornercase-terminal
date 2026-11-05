#![cfg(unix)]

use super::*;
use crate::protocol::{MachineOrder, Machines};

const WINDOW: u64 = 7;

fn reaches(app: &mut App, here: Option<&str>, known: &[&str]) {
    let known = known.iter().map(|&name| name.to_string()).collect();
    app.set_machines(WINDOW, Some(Machines { here: here.map(Into::into), known }));
}

fn machine_button(app: &App) -> Position {
    let label = app.machine_label().expect("the button shows");
    let r = ui::machine_button(app_areas(app).worktrees, &label).0;
    Position::new(r.x + 1, r.y)
}

fn open_menu(app: &mut App) -> Vec<String> {
    let at = machine_button(app);
    click(app, at);
    menu_labels(app)
}

fn switch(to: Option<&str>) -> (u64, MachineOrder) {
    (WINDOW, MachineOrder::Switch(to.map(Into::into)))
}

mod button {
    use super::*;

    #[test]
    fn is_hidden_while_the_window_knows_no_other_machine() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &[]);

        assert_eq!(app.machine_label(), None);
    }

    #[test]
    fn is_hidden_for_a_window_that_cannot_switch() {
        let (mut app, _rx) = app();
        app.set_machines(WINDOW, None);

        assert_eq!(app.machine_label(), None);
    }

    #[test]
    fn names_the_machine_the_window_is_on() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox"]);
        let here = app.machine_label();
        reaches(&mut app, Some("devbox"), &[]);

        assert_eq!([here, app.machine_label()], [Some("this machine".into()), Some("devbox".into())]);
    }

    #[test]
    fn worktrees_still_opens_from_the_left_of_the_row() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox"]);
        let row = app_areas(&app).worktrees;

        click(&mut app, Position::new(row.x + 2, row.y));

        assert!(matches!(app.overlay, Some(Overlay::Worktrees(_))));
    }

    #[test]
    fn the_icon_bar_opens_the_same_menu() {
        let (mut app, _rx) = app();
        app.config.icon_bar = true;
        reaches(&mut app, None, &["devbox"]);
        let bar = app.layout(AREA).activity;
        let (_, r) = ui::activity_items(bar, app.activity_shown())
            .into_iter()
            .find(|(item, _)| *item == ui::ActivityItem::Machine)
            .expect("the machine slot");

        click(&mut app, Position::new(r.x + 1, r.y));

        assert_eq!(menu_labels(&app)[0], "✓ this machine");
    }
}

mod menu {
    use super::*;

    #[test]
    fn lists_this_machine_then_the_known_ones_and_marks_the_current_one() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox", "gpu-box"]);

        assert_eq!(open_menu(&mut app), ["✓ this machine", "  devbox", "  gpu-box", "add machine…", "forget…"]);
    }

    #[test]
    fn the_machine_it_is_on_is_listed_once_forgotten_and_cannot_be_forgotten_again() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &[]);

        assert_eq!(open_menu(&mut app), ["  this machine", "✓ devbox", "add machine…"]);
    }

    #[test]
    fn picking_a_machine_tells_the_window_that_showed_the_menu() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &["devbox", "gpu-box"]);
        open_menu(&mut app);

        pick(&mut app, "  this machine");

        assert_eq!(app.take_machine_orders(), [switch(None)]);
    }

    #[test]
    fn picking_the_machine_it_is_on_does_nothing() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &["devbox"]);
        open_menu(&mut app);

        pick(&mut app, "✓ devbox");

        assert_eq!(app.take_machine_orders(), []);
    }

    #[test]
    fn forget_lists_the_known_machines() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &["devbox", "gpu-box"]);
        open_menu(&mut app);
        pick(&mut app, "forget…");
        let labels = menu_labels(&app);

        pick(&mut app, "forget gpu-box");

        assert_eq!(labels, ["forget devbox", "forget gpu-box"]);
        assert_eq!(app.take_machine_orders(), [(WINDOW, MachineOrder::Forget("gpu-box".into()))]);
    }
}

mod adding {
    use super::*;

    fn add(app: &mut App) {
        reaches(app, None, &["devbox"]);
        open_menu(app);
        pick(app, "add machine…");
    }

    fn error(app: &App) -> Option<String> {
        match &app.overlay {
            Some(Overlay::NewMachine { error, .. }) => error.clone(),
            _ => panic!("the dialog closed"),
        }
    }

    #[test]
    fn connecting_tells_the_window_where_to_go() {
        let (mut app, _rx) = app();
        add(&mut app);

        submit_text(&mut app, "  me@new-box ");

        assert_eq!((app.take_machine_orders(), app.overlay.is_none()), ([switch(Some("me@new-box"))].into(), true));
    }

    #[test]
    fn an_empty_destination_keeps_the_dialog_open() {
        let (mut app, _rx) = app();
        add(&mut app);

        submit_text(&mut app, "  ");

        assert_eq!((error(&app), app.take_machine_orders()), (None, Vec::new()));
    }

    #[test]
    fn a_destination_ssh_would_read_as_an_option_is_refused() {
        let (mut app, _rx) = app();
        add(&mut app);

        submit_text(&mut app, "-oProxyCommand=x");

        assert!(error(&app).is_some_and(|error| error.contains("does not start with -")));
        assert_eq!(app.take_machine_orders(), []);
    }

    #[test]
    fn typing_again_clears_the_error() {
        let (mut app, _rx) = app();
        add(&mut app);
        submit_text(&mut app, "two words");

        type_text(&mut app, "x");

        assert_eq!(error(&app), None);
    }
}
