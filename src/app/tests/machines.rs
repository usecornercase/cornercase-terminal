#![cfg(unix)]

use super::*;
use crate::host::open::Browse;
use crate::protocol::{Holding, MachineOrder, Machines, Tunnel, TunnelOrder, TunnelState};

const WINDOW: u64 = 7;

fn reaches(app: &mut App, here: Option<&str>, known: &[&str]) {
    let known = known.iter().map(|&name| name.to_string()).collect();
    app.set_machines(WINDOW, Some(Machines { here: here.map(Into::into), known }));
}

fn machine_button(app: &App) -> Position {
    let label = app.machine_label().expect("the button shows");
    let r = ui::machine_button(app_areas(app).machine, &label, app.machine_tunnels()).0;
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

mod tunnels {
    use super::*;

    fn holding(app: &mut App, browse: bool, tunnels: &[(u16, Option<u16>, TunnelState)]) {
        let tunnels =
            tunnels.iter().map(|(port, local, state)| Tunnel { port: *port, local: *local, state: state.clone() });
        let orders = app.opening.tunnels.sent(WINDOW);
        app.tunnels_reported(WINDOW, Holding { browse, orders, tunnels: tunnels.collect() });
    }

    fn mixed(app: &mut App) {
        reaches(app, Some("devbox"), &[]);
        let tunnels = [
            (5173, Some(5174), TunnelState::Up),
            (8080, None, TunnelState::Down("refused".into())),
            (3000, None, TunnelState::Closed),
        ];
        holding(app, true, &tunnels);
    }

    fn toast(app: &App) -> String {
        app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default()
    }

    #[test]
    fn the_button_counts_the_ones_that_are_not_closed() {
        let (mut app, _rx) = app();
        mixed(&mut app);

        assert_eq!(app.machine_tunnels(), 2);
    }

    #[test]
    fn the_menu_lists_them_by_port_after_the_machines() {
        let (mut app, _rx) = app();
        mixed(&mut app);

        let expected = [
            "  this machine",
            "✓ devbox",
            "add machine…",
            "⇄ 3000  closed",
            "⇄ 5173 → localhost:5174  up",
            "⇄ 8080  down",
        ];
        assert_eq!(open_menu(&mut app), expected);
    }

    #[test]
    fn a_window_that_holds_none_lists_none() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &[]);
        holding(&mut app, true, &[]);

        assert_eq!(open_menu(&mut app), ["  this machine", "✓ devbox", "add machine…"]);
    }

    #[test]
    fn each_state_offers_its_own_verbs() {
        let verbs = |row: &str| {
            let (mut app, _rx) = app();
            mixed(&mut app);
            open_menu(&mut app);
            pick(&mut app, row);
            menu_labels(&app)
        };

        assert_eq!(verbs("⇄ 5173 → localhost:5174  up"), ["open in browser", "close", "forget"]);
        assert_eq!(verbs("⇄ 3000  closed"), ["open", "forget"]);
        assert_eq!(verbs("⇄ 8080  down"), ["open", "close", "forget"]);
    }

    #[test]
    fn a_window_that_cannot_browse_does_not_offer_it() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &[]);
        holding(&mut app, false, &[(5173, Some(5174), TunnelState::Up)]);

        open_menu(&mut app);
        pick(&mut app, "⇄ 5173 → localhost:5174  up");

        assert_eq!(menu_labels(&app), ["close", "forget"]);
    }

    #[test]
    fn picking_close_orders_it_and_says_so() {
        let (mut app, _rx) = app();
        mixed(&mut app);
        open_menu(&mut app);
        pick(&mut app, "⇄ 5173 → localhost:5174  up");

        pick(&mut app, "close");

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Close(5173))]);
        assert_eq!(toast(&app), "closed the tunnel of port 5173");
    }

    #[test]
    fn picking_open_asks_the_window() {
        let (mut app, _rx) = app();
        mixed(&mut app);
        open_menu(&mut app);
        pick(&mut app, "⇄ 3000  closed");

        pick(&mut app, "open");

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Open(3000))]);
    }

    #[test]
    fn picking_forget_orders_it_and_says_so() {
        let (mut app, _rx) = app();
        mixed(&mut app);
        open_menu(&mut app);
        pick(&mut app, "⇄ 5173 → localhost:5174  up");

        pick(&mut app, "forget");

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Forget(5173))]);
        assert_eq!(toast(&app), "forgot the tunnel of port 5173");
    }

    #[test]
    fn picking_open_in_browser_asks_the_window_to_browse() {
        let (mut app, _rx) = app();
        mixed(&mut app);
        open_menu(&mut app);
        pick(&mut app, "⇄ 5173 → localhost:5174  up");

        pick(&mut app, "open in browser");

        assert_eq!(app.take_browses(), [(WINDOW, Browse { ip: None, port: 5173 })]);
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
