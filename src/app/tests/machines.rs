#![cfg(unix)]

use super::*;
use crate::host::open::Browse;
use crate::machines::form::{ADD, BACK, Field, Form};
use crate::machines::{Machine, machine};
use crate::protocol::{Holding, MachineOrder, Machines, Tunnel, TunnelOrder, TunnelState};

const WINDOW: u64 = 7;

fn listed(app: &mut App, names: &[&str]) {
    app.config.machines = names.iter().map(|name| machine(name, &format!("{name}.lan"))).collect();
}

fn reaches(app: &mut App, here: Option<&str>, known: &[&str]) {
    listed(app, known);
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
    fn is_hidden_while_the_list_is_empty_and_the_window_is_on_this_computer() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &[]);

        assert_eq!(app.machine_label(), None);
    }

    #[test]
    fn shows_once_the_list_holds_a_machine_shown_or_not() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &[]);
        app.config.machines = vec![Machine { shown: false, ..machine("devbox", "devbox.lan") }];

        assert_eq!(app.machine_label(), Some("this computer".into()));
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

        assert_eq!([here, app.machine_label()], [Some("this computer".into()), Some("on devbox".into())]);
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

        assert_eq!(menu_labels(&app)[0], "✓ this computer");
    }
}

mod menu {
    use super::*;

    #[test]
    fn lists_this_computer_then_the_listed_machines_and_the_two_ways_to_manage_them() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox", "gpu-box"]);

        assert_eq!(
            open_menu(&mut app),
            ["✓ this computer", "  devbox", "  gpu-box", "add machine…", "manage machines…"]
        );
    }

    #[test]
    fn on_this_computer_it_is_built_from_the_servers_own_list_not_the_windows() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox"]);
        listed(&mut app, &["newbox", "devbox"]);

        assert_eq!(
            open_menu(&mut app),
            ["✓ this computer", "  newbox", "  devbox", "add machine…", "manage machines…"]
        );
    }

    #[test]
    fn on_another_machine_it_uses_the_names_the_window_sent_and_adds_none() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &["vps", "devbox"]);

        assert_eq!(open_menu(&mut app), ["  this computer", "  vps", "✓ devbox"]);
    }

    #[test]
    fn the_machine_it_is_on_is_listed_even_when_the_window_did_not_send_it() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &[]);

        assert_eq!(open_menu(&mut app), ["  this computer", "✓ devbox"]);
    }

    #[test]
    fn manage_machines_opens_the_machines_page() {
        let (mut app, _rx) = app();
        reaches(&mut app, None, &["devbox"]);
        open_menu(&mut app);

        pick(&mut app, "manage machines…");

        assert!(matches!(&app.overlay, Some(Overlay::Settings(s)) if s.page == Page::Machines));
    }

    #[test]
    fn picking_a_machine_tells_the_window_that_showed_the_menu() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &["devbox", "gpu-box"]);
        open_menu(&mut app);

        pick(&mut app, "  this computer");

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

        let expected = ["  this computer", "✓ devbox", "⇄ 3000  closed", "⇄ 5173 → localhost:5174  up", "⇄ 8080  down"];
        assert_eq!(open_menu(&mut app), expected);
    }

    #[test]
    fn a_window_that_holds_none_lists_none() {
        let (mut app, _rx) = app();
        reaches(&mut app, Some("devbox"), &[]);
        holding(&mut app, true, &[]);

        assert_eq!(open_menu(&mut app), ["  this computer", "✓ devbox"]);
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

    fn form_from_the_button(app: &mut App) {
        reaches(app, None, &["devbox"]);
        open_menu(app);
        pick(app, "add machine…");
    }

    fn form(app: &App) -> &Form {
        let Some(Overlay::Machine(form)) = &app.overlay else { panic!("the machine form is not open") };
        form
    }

    fn press(app: &mut App, label: &str) {
        let labels = form(app).buttons();
        click_button(app, AREA, &labels, &ui::machine_form::buttons(AREA, &labels), label);
    }

    #[test]
    fn opens_the_form_of_settings_that_goes_back_to_the_window() {
        let (mut app, _rx) = app();

        form_from_the_button(&mut app);

        assert!(!form(&app).back_to_settings);
    }

    #[test]
    fn back_closes_it() {
        let (mut app, _rx) = app();
        form_from_the_button(&mut app);

        press(&mut app, BACK);

        assert!(app.overlay.is_none());
    }

    #[test]
    fn saving_closes_it_with_a_toast_and_writes_the_machine_as_ticked() {
        let (mut app, _rx) = app();
        form_from_the_button(&mut app);
        click(&mut app, ui::machine_form::text(AREA, Field::Name).as_position());
        type_text(&mut app, "newbox");
        click(&mut app, ui::machine_form::text(AREA, Field::Destination).as_position());
        type_text(&mut app, "new.lan");
        click(&mut app, ui::machine_form::text(AREA, Field::Shown).as_position());

        press(&mut app, ADD);

        let added = app.config.machines.last().expect("the machine");
        assert_eq!((added.name.as_str(), added.shown, added.unreached), ("newbox", false, false));
        assert!(app.overlay.is_none());
        assert_eq!(app.messages.toast.as_ref().map(|toast| toast.message.as_str()), Some("newbox added"));
    }
}
