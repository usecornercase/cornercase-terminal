use ratatui::style::Color;

use super::*;
use crate::app::machine_form::CHANGED;
use crate::machines::Status;
use crate::machines::form::{ADD, BACK, Field, Form};
use crate::settings::{Page, Row};

const WIDE: Rect = Rect { x: 0, y: 0, width: 144, height: 42 };
const NARROW: Rect = Rect { x: 0, y: 0, width: 80, height: 24 };

impl Rig {
    fn machines_page(&mut self) {
        from_more(&mut self.app, "settings");
        let tabs = ui::settings_tabs(ui::settings_area(AREA), &Page::ALL.map(Page::name));
        click(&mut self.app, tabs[Page::Machines.index()].as_position());
    }

    fn page(&self) -> &Settings {
        let Some(Overlay::Settings(s)) = &self.app.overlay else { panic!("the settings are not open") };
        s
    }

    fn row_at(&self, row: &Row) -> Rect {
        let s = self.page();
        let rows = s.rows();
        let i = rows.iter().position(|r| r == row).expect("the row is there");
        let sections: Vec<&str> = rows.iter().map(Row::section).collect();
        ui::settings_row(AREA, &sections, s.cursor, i)
    }

    fn click_row(&mut self, row: &Row) {
        let pos = self.row_at(row).as_position();
        click(&mut self.app, pos);
    }

    fn selected(&self) -> Row {
        self.page().rows()[self.page().cursor].clone()
    }

    fn form(&self) -> &Form {
        let Some(Overlay::Machine(form)) = &self.app.overlay else { panic!("the machine form is not open") };
        form
    }

    fn click_field(&mut self, field: Field) {
        click(&mut self.app, ui::machine_form::text(AREA, field).as_position());
    }

    fn press_button(&mut self, label: &str) {
        let labels = self.form().buttons();
        click_button(&mut self.app, AREA, &labels, &ui::machine_form::buttons(AREA, &labels), label);
    }

    fn shown(&mut self, area: Rect) -> String {
        render(&mut self.app, area).backend().to_string().replace(version::CURRENT, "<version>")
    }
}

fn on_the_page(machines: Vec<Machine>) -> Rig {
    let mut rig = Rig::new(machines, There::Missing);
    rig.machines_page();
    rig
}

fn status(state: State) -> Status {
    let version = matches!(state, State::Linked | State::Older).then(|| "0.12.200".to_string());
    Status { state, server_version: version.clone(), version, ..Status::default() }
}

fn every_state() -> Rig {
    let machines = vec![
        machine("vps", "miguel@vps.example.com"),
        Machine { icon: "▣".into(), folder: "/srv/code".into(), ..machine("mini", "mac-mini.local") },
        Machine { icon: "◫".into(), ..machine("pi", "pi.lan") },
        machine("new", "new.lan"),
        machine("my vps", "box.lan"),
    ];
    let mut rig = Rig::new(machines, There::Missing);
    let states = [State::Linked, State::Unreachable, State::Older, State::Connecting];
    for (at, state) in states.into_iter().enumerate() {
        rig.app.machines.set_status(at, status(state));
    }
    rig.machines_page();
    rig
}

mod the_page {
    use super::*;

    #[test]
    fn is_the_last_tab_and_lists_this_computer_first() {
        let rig = on_the_page(vec![machine("vps", "vps.lan")]);

        assert_eq!(
            (rig.page().page, rig.page().rows()),
            (Page::Machines, vec![Row::Here, Row::Machine(0), Row::AddMachine])
        );
    }

    #[test]
    fn is_drawn_with_each_state_at_the_settings_full_size() {
        insta::assert_snapshot!(every_state().shown(WIDE));
    }

    #[test]
    fn is_drawn_with_each_state_on_a_narrow_window() {
        insta::assert_snapshot!(every_state().shown(NARROW));
    }

    #[test]
    fn colours_each_mark_by_its_state() {
        let mut rig = every_state();
        let t = render(&mut rig.app, AREA);
        let colour = |row: &Row| {
            let r = rig.row_at(row);
            t.backend().buffer()[(r.x + 2, r.y)].fg
        };

        let marks = [Row::Machine(0), Row::Machine(1), Row::Machine(2)].map(|row| colour(&row));
        assert_eq!(marks, [Color::Green, ui::muted(rig.app.themes.host()), Color::Yellow]);
    }

    #[test]
    fn shows_the_selected_machine_s_reason_on_the_hint_line() {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);
        rig.machines_page();

        send_key(&mut rig.app, KeyCode::Down, KeyModifiers::NONE);

        assert!(rig.shown(AREA).contains("cornercase was not found on vps: install it there"));
    }

    #[test]
    fn follows_the_links_while_it_is_open() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        assert_eq!(rig.page().machine_info(0).map(|info| info.state), Some(State::Connecting));

        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);
        rig.refresh(Duration::from_millis(500));

        assert_eq!(rig.page().machine_info(0).map(|info| info.state), Some(State::Unreachable));
    }

    #[test]
    fn keeps_the_selected_row_when_a_machine_above_turns_older() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        send_key(&mut rig.app, KeyCode::Down, KeyModifiers::NONE);
        send_key(&mut rig.app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(rig.selected(), Row::AddMachine);

        rig.app.machines.set_status(0, status(State::Older));
        rig.refresh(Duration::from_millis(500));

        assert_eq!((rig.page().rows().len(), rig.selected()), (4, Row::AddMachine));
    }

    #[test]
    fn shows_a_machine_added_outside() {
        let mut rig = on_the_page(Vec::new());

        rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(2));

        assert_eq!(rig.page().rows(), [Row::Here, Row::Machine(0), Row::AddMachine]);
    }

    #[test]
    fn this_computer_does_nothing_on_enter() {
        let mut rig = on_the_page(Vec::new());

        send_key(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!((rig.selected(), rig.page().notice.clone()), (Row::Here, None));
    }
}

mod the_form {
    use super::*;

    #[test]
    fn add_opens_it_with_the_server_icon_and_the_code_folder() {
        let mut rig = on_the_page(Vec::new());

        rig.click_row(&Row::AddMachine);

        let form = rig.form();
        assert_eq!(
            (form.title().as_str(), form.icon.label(), form.folder.text().as_str()),
            ("Settings › Machines › new machine", Some("☁ server"), "~/code")
        );
        insta::assert_snapshot!(rig.shown(WIDE));
    }

    #[test]
    fn saves_what_was_typed_and_goes_back_to_the_machines() {
        let mut rig = on_the_page(Vec::new());
        rig.click_row(&Row::AddMachine);

        type_text(&mut rig.app, "vps");
        rig.click_field(Field::Destination);
        type_text(&mut rig.app, "me@vps.lan");
        rig.press_button(ADD);

        let added =
            Machine {
                name: "vps".into(), icon: "☁".into(), destination: "me@vps.lan".into(), folder: "~/code".into()
            };
        assert_eq!(rig.saved(), [added]);
        assert_eq!((rig.selected(), rig.page().notice.as_deref()), (Row::Machine(0), Some("vps added")));
    }

    #[test]
    fn a_click_on_a_machine_opens_it_filled_in_and_enter_saves_it() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")]);
        rig.click_row(&Row::Machine(1));
        assert_eq!(rig.form().title(), "Settings › Machines › pi");

        rig.click_field(Field::Icon);
        let list = ui::machine_form::list(AREA, rig.form()).expect("the icons are listed");
        click(&mut rig.app, Position::new(list.x + 2, list.y + 3));
        rig.click_field(Field::Name);
        send_key(&mut rig.app, KeyCode::End, KeyModifiers::NONE);
        type_text(&mut rig.app, "2");
        send_key(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

        let edited = Machine { name: "pi2".into(), icon: "▣".into(), ..machine("pi", "pi.lan") };
        assert_eq!(rig.saved(), [machine("vps", "vps.lan"), edited]);
        assert_eq!((rig.selected(), rig.page().notice.as_deref()), (Row::Machine(1), Some("pi2 saved")));
    }

    #[test]
    fn saves_the_machine_it_opened_after_an_edit_outside_that_was_not_read_yet() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")]);
        rig.click_row(&Row::Machine(0));
        let outside = vec![machine("mini", "mini.local"), machine("vps", "vps.lan"), machine("pi", "pi.lan")];
        write_config(rig.dir.path(), &Config { machines: outside, ..Config::default() });

        rig.click_field(Field::Name);
        send_key(&mut rig.app, KeyCode::End, KeyModifiers::NONE);
        type_text(&mut rig.app, "2");
        send_key(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

        let renamed = Machine { name: "vps2".into(), ..machine("vps", "vps.lan") };
        assert_eq!(rig.saved(), [machine("mini", "mini.local"), renamed, machine("pi", "pi.lan")]);
    }

    #[test]
    fn refuses_a_machine_changed_outside_since_it_opened() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        rig.click_row(&Row::Machine(0));
        let outside = vec![machine("vps", "vps.example.com")];
        write_config(rig.dir.path(), &Config { machines: outside.clone(), ..Config::default() });

        rig.click_field(Field::Name);
        send_key(&mut rig.app, KeyCode::End, KeyModifiers::NONE);
        type_text(&mut rig.app, "2");
        send_key(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(rig.saved(), outside);
        assert_eq!(rig.form().notice.as_deref(), Some(CHANGED));
    }

    #[test]
    fn enter_on_a_machine_opens_it_too() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);

        send_key(&mut rig.app, KeyCode::Down, KeyModifiers::NONE);
        send_key(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(rig.form().title(), "Settings › Machines › vps");
    }

    #[test]
    fn back_returns_to_the_machine_with_nothing_written() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        rig.click_row(&Row::Machine(0));
        type_text(&mut rig.app, "zzz");

        rig.press_button(BACK);

        assert_eq!((rig.page().page, rig.selected()), (Page::Machines, Row::Machine(0)));
        assert_eq!(rig.saved(), [machine("vps", "vps.lan")]);
    }

    #[test]
    fn a_refusal_is_said_under_its_field_and_writes_nothing() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        rig.click_row(&Row::AddMachine);
        type_text(&mut rig.app, "pi");
        rig.click_field(Field::Destination);
        type_text(&mut rig.app, "vps.lan");

        rig.press_button(ADD);

        assert_eq!(rig.form().error(Field::Destination), Some("the machine `vps` already uses this destination"));
        assert_eq!(rig.saved(), [machine("vps", "vps.lan")]);
        insta::assert_snapshot!(rig.shown(WIDE));
    }

    #[test]
    fn keeps_every_field_on_a_short_window() {
        let short = Rect { x: 0, y: 0, width: 80, height: 18 };

        let fields = Field::ALL.map(|field| ui::machine_form::field(short, field));

        assert!(fields.iter().all(|field| field.height == 1), "{fields:?}");
        assert!(fields.windows(2).all(|pair| pair[0].y < pair[1].y), "{fields:?}");
    }

    #[test]
    fn its_icons_open_below_their_field() {
        let mut rig = on_the_page(Vec::new());
        rig.click_row(&Row::AddMachine);

        rig.click_field(Field::Icon);

        assert!(rig.form().icon.is_open());
        insta::assert_snapshot!(rig.shown(WIDE));
    }
}

mod removing {
    use super::*;

    fn cross(rig: &Rig, row: &Row) -> Position {
        ui::settings_remove(rig.row_at(row)).as_position()
    }

    #[test]
    fn a_machine_s_cross_removes_it_at_once_and_says_its_server_keeps_running() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan"), machine("pi", "pi.lan")]);

        let at = cross(&rig, &Row::Machine(0));
        click(&mut rig.app, at);

        assert_eq!(rig.saved(), [machine("pi", "pi.lan")]);
        assert_eq!(rig.page().notice.as_deref(), Some("vps removed: its server keeps running there"));
    }

    #[test]
    fn of_a_machine_changed_outside_removes_nothing_and_says_so() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        let outside = vec![Machine { icon: "▣".into(), ..machine("vps", "vps.lan") }];
        write_config(rig.dir.path(), &Config { machines: outside.clone(), ..Config::default() });

        let at = cross(&rig, &Row::Machine(0));
        click(&mut rig.app, at);

        assert_eq!(rig.saved(), outside);
        assert_eq!(rig.page().notice.as_deref(), Some("vps changed outside the settings: nothing was removed"));
    }

    #[test]
    fn keeps_what_changed_outside_in_another_key() {
        let mut rig = on_the_page(vec![machine("vps", "vps.lan")]);
        let outside = Config { machines: vec![machine("vps", "vps.lan")], sidebar: "tree".into(), ..Config::default() };
        write_config(rig.dir.path(), &outside);

        let at = cross(&rig, &Row::Machine(0));
        click(&mut rig.app, at);

        let read = config::load(&rig.dir.path().join("config.json"));
        assert_eq!((read.machines, read.sidebar.as_str()), (Vec::new(), "tree"));
    }
}

mod updating {
    use super::*;

    #[test]
    fn the_row_under_an_older_machine_runs_it_once_at_a_time() {
        let mut rig = Rig::new(vec![machine("pi", "pi.lan")], There::Older);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Older);
        let (go, asked) = rig.gated_updates();
        rig.machines_page();

        rig.click_row(&Row::UpdateMachine(0));
        assert!(rig.page().machine_info(0).is_some_and(|info| info.updating));
        rig.click_row(&Row::UpdateMachine(0));
        go.send(()).expect("let the update finish");

        rig.until("the update ends", |app| !app.machines.held()[0].status.updating);
        assert_eq!(*asked.lock(), ["pi.lan"]);
    }

    #[test]
    fn right_after_a_destination_edit_never_reaches_the_old_one() {
        let mut rig = Rig::new(vec![machine("pi", "pi.lan")], There::Older);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Older);
        let (_go, _asked) = rig.gated_updates();
        rig.app.change_config(|config| config.machines[0].destination = "new.lan".into()).expect("edited");

        rig.app.machine_action(&crate::settings::Action::UpdateMachine(0));

        assert!(rig.app.machines.held().iter().all(|held| !held.status.updating && held.destination == "new.lan"));
    }
}
