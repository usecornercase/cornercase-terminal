use super::*;
use crate::app::toast::{TunnelButton, UNDO_FOR};
use crate::app::tunnels::Origin;
use crate::protocol::{Holding, Machines, Tunnel, TunnelOrder, TunnelState};
use crate::ui::PortTunnel;

const WINDOW: u64 = 7;

fn remote_window(app: &mut App) {
    app.set_machines(WINDOW, Some(Machines { here: Some("vps".into()), known: Vec::new() }));
}

fn holding(app: &mut App, browse: bool, tunnels: Vec<(u16, Option<u16>, TunnelState)>) {
    let tunnels = tunnels.into_iter().map(|(port, local, state)| Tunnel { port, local, state }).collect();
    let orders = app.opening.tunnels.sent(WINDOW);
    app.tunnels_reported(WINDOW, Holding { browse, orders, tunnels });
}

fn remote_with(browse: bool, tunnels: Vec<(u16, Option<u16>, TunnelState)>) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    remote_window(&mut app);
    holding(&mut app, browse, tunnels);
    (app, rx, dirs)
}

fn showing(app: &mut App, ports: Vec<Port>) {
    let generation = opened(app);
    listed(app, generation, ports);
}

fn first_row(app: &App) -> ui::PortRow {
    let Some(Overlay::Ports(dialog)) = &app.overlay else { panic!("the ports dialog is not open") };
    app.ports_view(dialog, AREA).rows.remove(0)
}

fn click_row(app: &mut App, hit: Hit) {
    let action = dialog(app).click(hit);
    app.ports_act(action);
}

fn toast(app: &App) -> String {
    app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default()
}

fn lan_port(pid: i32, number: u16) -> Port {
    Port { addresses: vec!["192.168.1.5".parse().expect("an address")], ..port(pid, number) }
}

mod the_row {
    use super::*;

    #[test]
    fn of_a_closed_tunnel_offers_one_again() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![(5173, Some(5174), TunnelState::Closed)]);
        showing(&mut app, vec![port(10, 5173)]);

        assert_eq!(first_row(&app).tunnel, Some(PortTunnel::Offered));
    }

    #[test]
    fn of_a_remote_window_offers_a_tunnel_and_asks_for_it_when_clicked() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![]);
        showing(&mut app, vec![port(10, 5173)]);

        assert_eq!(first_row(&app).tunnel, Some(PortTunnel::Offered));
        click_row(&mut app, Hit::Tunnel(0));

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Open(5173))]);
    }

    #[test]
    fn that_is_up_offers_open_copy_and_close() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![(5173, Some(5174), TunnelState::Up)]);
        showing(&mut app, vec![port(10, 5173)]);

        let row = first_row(&app);
        assert_eq!((row.tunnel, row.open), (Some(PortTunnel::Up(5174)), true));
        click_row(&mut app, Hit::Open(0));
        assert_eq!(app.take_browses(), [(WINDOW, Browse { ip: None, port: 5173 })]);
        click_row(&mut app, Hit::Copy(0));
        assert_eq!(toast(&app), "copied http://localhost:5174");
        click_row(&mut app, Hit::CloseTunnel(0));
        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Close(5173))]);
    }

    #[test]
    fn that_is_opening_offers_only_to_close_it() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![(5173, Some(5173), TunnelState::Opening)]);
        showing(&mut app, vec![port(10, 5173)]);

        let row = first_row(&app);

        assert_eq!((row.tunnel, row.open), (Some(PortTunnel::Opening), false));
    }

    #[test]
    fn of_a_window_that_cannot_browse_has_no_open() {
        let (mut app, _rx, _dirs) = remote_with(false, vec![(5173, Some(5173), TunnelState::Up)]);
        showing(&mut app, vec![port(10, 5173)]);

        assert!(!first_row(&app).open);
    }

    #[test]
    fn of_a_local_window_keeps_open_and_has_no_tunnel() {
        let (mut app, _rx, _dirs) = app_with(1);
        app.set_window(WINDOW, &[Opener::Default]);
        showing(&mut app, vec![port(10, 5173)]);

        let row = first_row(&app);

        assert_eq!((row.tunnel, row.open), (None, true));
    }

    #[test]
    fn listening_on_a_lan_address_has_no_tunnel() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![]);
        showing(&mut app, vec![lan_port(10, 5173)]);

        assert_eq!(first_row(&app).tunnel, None);
    }
}

mod the_toast {
    use super::*;

    #[test]
    fn of_a_clicked_tunnel_says_which_port_it_took_and_opens_it() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![]);
        showing(&mut app, vec![port(10, 5173)]);
        click_row(&mut app, Hit::Tunnel(0));
        app.take_tunnel_orders();

        holding(&mut app, true, vec![(5173, Some(5174), TunnelState::Up)]);

        assert_eq!(toast(&app), "port 5173 is tunnelled to localhost:5174 (5173 is taken here)");
        let shown = app.messages.toast.as_ref().expect("a toast");
        assert_eq!(shown.tunnel, Some(TunnelButton::Open(5173)));
        let button = ui::toast_button(AREA, shown.view());
        click(&mut app, Position::new(button.x, button.y));
        assert_eq!(app.take_browses(), [(WINDOW, Browse { ip: None, port: 5173 })]);
    }

    #[test]
    fn of_a_clicked_tunnel_in_a_window_that_cannot_browse_still_lasts_long_enough_to_read() {
        let (mut app, _rx, _dirs) = remote_with(false, vec![]);
        app.order_tunnel(WINDOW, TunnelOrder::Open(5173), Some(Origin::Click));

        holding(&mut app, false, vec![(5173, Some(5174), TunnelState::Up)]);

        let shown = app.messages.toast.as_ref().expect("a toast");
        assert_eq!((shown.tunnel, shown.lasts()), (None, UNDO_FOR));
    }

    #[test]
    fn of_a_clicked_tunnel_on_the_same_port_does_not_say_it_was_taken() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![]);
        app.order_tunnel(WINDOW, TunnelOrder::Open(5173), Some(Origin::Click));

        holding(&mut app, true, vec![(5173, Some(5173), TunnelState::Up)]);

        assert_eq!(toast(&app), "port 5173 is tunnelled to localhost:5173");
    }

    #[test]
    fn of_a_requested_tunnel_offers_to_close_it() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![]);
        app.order_tunnel(WINDOW, TunnelOrder::Open(5173), Some(Origin::Request));

        holding(&mut app, true, vec![(5173, Some(5174), TunnelState::Up)]);

        assert_eq!(toast(&app), "a program tunnelled port 5173 to localhost:5174");
        let shown = app.messages.toast.as_ref().expect("a toast");
        assert_eq!(shown.tunnel, Some(TunnelButton::Close(5173)));
    }

    #[test]
    fn of_a_tunnel_that_fails_to_come_up_says_why_whoever_asked() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![(5173, Some(5173), TunnelState::Opening)]);

        holding(&mut app, true, vec![(5173, Some(5173), TunnelState::Down("Permission denied".into()))]);

        assert_eq!(toast(&app), "could not tunnel port 5173: Permission denied");
    }

    #[test]
    fn a_tunnel_that_was_up_and_drops_shows_nothing() {
        let (mut app, _rx, _dirs) = remote_with(true, vec![(5173, Some(5173), TunnelState::Up)]);

        holding(&mut app, true, vec![(5173, Some(5173), TunnelState::Down("reset".into()))]);

        assert_eq!(toast(&app), "");
    }
}
