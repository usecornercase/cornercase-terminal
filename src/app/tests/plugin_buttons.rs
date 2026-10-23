use super::plugins::{Plugged, plugged_among};
use super::*;

const NOTE: &str = r#"id = "acme.notes"
name = "Notes"
version = "1.0"

[[actions]]
id = "note"
title = "take a note"
contexts = ["button"]
command = ["./act"]
"#;

const AGAIN: &str = r#"
[[actions]]
id = "again"
title = "note again"
contexts = ["button"]
command = ["./act"]
"#;

const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 40 };

fn other(id: &str, name: &str, out: &Path) -> String {
    format!(
        "id = \"{id}\"\nname = \"{name}\"\nversion = \"1.0\"\n\n[[actions]]\nid = \"go\"\ntitle = \"go\"\n\
         contexts = [\"button\"]\ncommand = [\"/bin/sh\", \"-c\", \"echo {id} >> '{}'\"]\n",
        out.display()
    )
}

fn row_button(app: &App, area: Rect, index: usize) -> Position {
    let faces = app.plugin_faces();
    let row =
        ui::plugin_buttons(app.layout(area).shown(app.screen.nav).plugins, faces.iter().map(|f| f.title.as_str()));
    row.buttons.get(index).map_or(row.more, |b| *b).as_position()
}

fn ran(p: &Plugged) -> String {
    wait_until("the action ran", || !p.read("out").is_empty());
    p.read("out")
}

mod row {
    use super::*;

    #[test]
    fn is_there_only_with_a_plugin_button() {
        let (with, without) = (plugged_among(true, NOTE, &[]), plugged_among(false, NOTE, &[]));
        assert_eq!((with.app.layout(AREA).plugins.height, without.app.layout(AREA).plugins.height), (1, 0));
    }

    #[test]
    fn a_click_runs_the_only_action() {
        let mut p = plugged_among(true, NOTE, &[]);
        let at = row_button(&p.app, AREA, 0);
        click(&mut p.app, at);
        assert!(p.app.overlay.is_none());
        assert!(ran(&p).starts_with("ctx=button action=note "), "{}", p.read("out"));
    }

    #[test]
    fn a_click_opens_the_menu_of_several_actions() {
        let mut p = plugged_among(true, &format!("{NOTE}{AGAIN}"), &[]);
        let at = row_button(&p.app, AREA, 0);
        click(&mut p.app, at);
        assert_eq!(menu_labels(&p.app), ["take a note", "note again"]);
        pick(&mut p.app, "note again");
        assert!(ran(&p).starts_with("ctx=button action=again "), "{}", p.read("out"));
    }

    #[test]
    fn the_more_button_lists_the_plugins_that_do_not_fit() {
        let out = TempDir::new();
        let file = out.path().join("ran");
        let others = [other("acme.deploy", "Deploy to production", &file), other("acme.logs", "Logs of today", &file)];
        let mut p = plugged_among(true, NOTE, &others);
        let at = row_button(&p.app, AREA, usize::MAX);
        click(&mut p.app, at);
        assert_eq!(menu_labels(&p.app), ["Deploy to production", "Logs of today"]);
        pick(&mut p.app, "Logs of today");
        wait_until("the hidden plugin ran", || std::fs::read_to_string(&file).is_ok_and(|s| s == "acme.logs\n"));
    }
}

mod icon_bar {
    use super::*;

    #[test]
    fn a_plugin_slot_runs_its_action() {
        let mut p = plugged_among(true, NOTE, &[]);
        p.app.config.icon_bar = true;
        let bar = p.app.layout(AREA).activity;
        let slot = ui::activity_items(bar, p.app.activity_shown())
            .into_iter()
            .find_map(|(item, r)| (item == ui::ActivityItem::Plugin(0)).then(|| Position::new(r.x + 1, r.y)))
            .expect("the plugin's slot");
        click(&mut p.app, slot);
        assert!(ran(&p).starts_with("ctx=button "), "{}", p.read("out"));
    }
}

mod compact {
    use super::*;

    #[test]
    fn the_projects_menu_runs_the_button_and_closes() {
        let mut p = plugged_among(true, NOTE, &[]);
        p.app.screen.nav = Some(ui::Nav::Projects);
        let at = row_button(&p.app, SMALL, 0);
        click_in(&mut p.app, at, SMALL);
        assert_eq!(p.app.screen.nav, None);
        assert!(ran(&p).starts_with("ctx=button "), "{}", p.read("out"));
    }
}
