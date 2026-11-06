use std::path::{Path, PathBuf};

use super::*;
use crate::app::plugins::Spot;
use crate::control::Ids;
use crate::control::plugins::ActionRan;
use crate::plugins::Registry;
use crate::plugins::manifest::FILE;
use crate::test_util::fake_program;

const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
const VIEWER: u64 = 31;

struct Plugged {
    app: App,
    rx: Receiver<AppEvent>,
    act: PathBuf,
    board: PathBuf,
    _folders: [TempDir; 4],
}

fn manifest(act: &Path, board: &Path) -> String {
    let (act, board) = (act.display(), board.display());
    format!(
        r#"id = "acme.notes"
name = "Notes"
version = "1.0"

[[actions]]
id = "tally"
title = "count"
contexts = ["button"]
command = ['{act}']

[[actions]]
id = "note"
title = "take a note"
contexts = ["pane"]
command = ['{act}']

[[actions]]
id = "board"
title = "show the board"
contexts = ["button", "pane"]
placement = "popup"
command = ['{board}']
"#
    )
}

fn plugged() -> Plugged {
    let folders = [TempDir::new(), TempDir::new(), TempDir::new(), TempDir::new()];
    let [config, plugin, viewed, windowed] = &folders;
    let (act, board) = (fake_program(plugin.path(), "act"), fake_program(plugin.path(), "board"));
    std::fs::write(plugin.path().join(FILE), manifest(&act, &board)).expect("manifest");
    std::fs::write(board.with_extension("hold"), "").expect("keep the popup's program running");
    let config = config.path().join("config.json");
    let mut registry = Registry::load(&paths::plugins(&config));
    registry.link(plugin.path()).expect("link");
    registry.set_enabled("acme.notes", true).expect("on");
    registry.save().expect("save");
    let (mut app, rx) = new_app_with_shell(SHELL, &config);
    app.open_project(viewed.path().to_path_buf(), AREA).expect("the project a view moves to");
    app.open_project(windowed.path().to_path_buf(), AREA).expect("the project the window shows");
    app.reload_plugins();
    Plugged { app, rx, act, board, _folders: folders }
}

impl Plugged {
    fn asks(&mut self, view: Option<u64>, command: &str, args: &Value) {
        let text = json!({"contract": 1, "view": view, "command": command, "args": args}).to_string();
        self.app.request(CLIENT, &text, Some(AREA), Instant::now());
    }

    fn reply(&mut self) -> std::result::Result<Value, Refused> {
        let mut answer = None;
        wait_until("the answer", || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            answer = self.app.take_answers().into_iter().next();
            answer.is_some()
        });
        reply_of(&answer.expect("an answer").1)
    }

    fn starts(&mut self, view: Option<u64>, action: &str, item: Option<Value>) {
        let mut args = json!({"plugin": "acme.notes", "action": action, "revision": self.app.plugin_revision()});
        if let Some(item) = item {
            args["item"] = item;
        }
        self.asks(view, "plugin-run", &args);
    }

    fn runs(
        &mut self,
        view: Option<u64>,
        action: &str,
        item: Option<Value>,
    ) -> std::result::Result<ActionRan, Refused> {
        self.starts(view, action, item);
        self.reply().map(|answer| serde_json::from_value(answer).expect("an outcome"))
    }

    fn opens(&mut self, args: &Value) -> u64 {
        let text = json!({"command": "view", "args": args}).to_string();
        self.app.request(VIEWER, &text, Some(AREA), Instant::now());
        let answers = self.app.take_answers();
        let opened: Value = serde_json::from_str(&answers[0].1).expect("a reply");
        opened["ok"]["view"]["id"].as_u64().expect("a view")
    }

    fn opens_on_the_other_project(&mut self, size: (u16, u16)) -> u64 {
        let project = self.app.projects[0].id;
        self.opens(&json!({"cols": size.0, "rows": size.1, "project": project}))
    }

    fn other_folder(&self) -> String {
        self.app.projects[0].workspaces[0].path.display().to_string()
    }

    fn window_pane(&self) -> u64 {
        self.app.shown_ids(self.app.active).pane.expect("the window's pane")
    }

    fn last_folder(program: &Path) -> String {
        let text = std::fs::read_to_string(program.with_extension("cwd")).expect("the program ran");
        text.lines().last().expect("a folder").to_string()
    }

    fn popup_size(&self) -> (u16, u16) {
        let (rows, cols) = self.app.popup().expect("a popup").term.size();
        (cols, rows)
    }

    fn opens_popup(&mut self, view: Option<u64>) {
        self.starts(view, "board", None);
        assert!(self.app.popup().is_some(), "the popup opens within the request");
    }

    fn popup_ended(&mut self) -> ActionRan {
        std::fs::remove_file(self.board.with_extension("hold")).expect("let the popup's program end");
        let answer = self.reply().expect("how it ended");
        serde_json::from_value(answer).expect("an outcome")
    }
}

mod a_button_action {
    use super::*;

    #[test]
    fn runs_on_what_the_view_shows_once_the_view_moved_to_another_project() {
        let mut p = plugged();
        let view = p.opens(&json!({"cols": 120, "rows": 40}));
        let other = p.app.projects[0].workspaces[0].id;
        p.asks(Some(view), "focus", &json!({"item": {"workspace": other}}));
        p.reply().expect("the view moved");
        let shown = p.app.view_ids(view);
        assert_eq!(shown.workspace, Some(other));

        let ran = p.runs(Some(view), "tally", None).expect("ran");

        assert_eq!((ran.context.as_str(), ran.ended.as_str(), ran.code), ("button", "exited", Some(0)));
        assert_eq!(ran.ids, shown);
        assert!(shown.tab.is_some() && shown.pane.is_some(), "{shown:?}");
        assert_eq!(Plugged::last_folder(&p.act), p.other_folder());
    }

    #[test]
    fn without_a_view_runs_on_what_the_window_shows_as_before() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        let window = p.app.shown_ids(p.app.active);

        let ran = p.runs(None, "tally", None).expect("ran");

        assert_eq!((ran.context.as_str(), ran.ids), ("button", window));
        assert_ne!(ran.ids, p.app.view_ids(view));
        assert_eq!(Plugged::last_folder(&p.act), p.app.projects[p.app.active].workspaces[0].path.display().to_string());
    }

    #[test]
    fn in_a_view_of_a_project_with_no_workspace_runs_in_the_projects_folder() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.app.projects[0].workspaces.clear();
        let (project, folder) = (p.app.projects[0].id, p.app.projects[0].path.display().to_string());

        let ran = p.runs(Some(view), "tally", None).expect("ran");

        assert_eq!(ran.ids, Ids { project: Some(project), ..Ids::default() });
        assert_eq!(Plugged::last_folder(&p.act), folder);
    }

    #[test]
    fn in_a_view_with_no_project_runs_in_the_home_folder_with_no_ids() {
        let config = TempDir::new();
        let (mut app, _rx) = new_app_with_shell(SHELL, config.path().join("config.json"));
        let open = json!({"command": "view", "args": {}}).to_string();
        app.request(VIEWER, &open, None, Instant::now());
        let opened: Value = serde_json::from_str(&app.take_answers()[0].1).expect("a reply");
        let view = opened["ok"]["view"]["id"].as_u64().expect("a view");
        let home = std::env::var_os(paths::HOME_VAR).map(PathBuf::from).expect("a home");

        assert_eq!(app.spot_env(Spot::ShownIn(view)), Some((Ids::default(), home)));
        assert_eq!(app.spot_env(Spot::ShownIn(view + 1)), None);
    }

    #[test]
    fn named_with_a_target_is_still_refused() {
        let mut p = plugged();
        let (view, pane) = (p.opens(&json!({})), p.window_pane());

        let refused = p.runs(Some(view), "tally", Some(json!({"pane": pane}))).expect_err("refused");

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("not on a pane"), "{}", refused.message);
    }
}

mod the_view {
    use super::*;

    #[test]
    fn is_not_found_when_it_closed_and_nothing_runs() {
        let mut p = plugged();
        let view = p.opens(&json!({}));
        p.app.forget(VIEWER);

        let refused = p.runs(Some(view), "tally", None).expect_err("refused");

        assert_eq!(refused.code, "not-found");
        assert!(refused.message.contains(&format!("there is no view {view}")), "{}", refused.message);
        assert!(!p.act.with_extension("args").exists());
    }

    #[test]
    fn gives_way_to_a_target_named_with_it() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        let pane = p.window_pane();

        let ran = p.runs(Some(view), "note", Some(json!({"pane": pane}))).expect("ran");

        assert_eq!((ran.context.as_str(), ran.ids.pane), ("pane", Some(pane)));
    }

    #[test]
    fn is_no_target_for_an_action_that_is_not_on_a_button() {
        let mut p = plugged();
        let view = p.opens(&json!({"cols": 120, "rows": 40}));

        let refused = p.runs(Some(view), "note", None).expect_err("refused");

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("only a button action"), "{}", refused.message);
        assert!(!p.act.with_extension("args").exists());
    }
}

mod a_popup_action {
    use super::*;

    #[test]
    fn without_a_view_or_a_window_is_refused_as_before() {
        let mut p = plugged();
        p.opens_on_the_other_project((120, 40));

        let refused = p.runs(None, "board", None).expect_err("refused");

        assert_eq!(refused.code, "failed");
        assert_eq!(refused.message, "no window is attached");
        assert!(p.app.popup().is_none());
    }

    #[test]
    fn opens_with_no_window_sized_from_the_view_on_what_it_shows() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        let shown = p.app.view_ids(view);

        p.opens_popup(Some(view));

        assert_eq!(p.popup_size(), (94, 30));
        let listed = p.app.app_state().popup.expect("the popup in the feed");
        assert_eq!((listed.action.as_str(), listed.cols, listed.rows), ("board", 94, 30));
        let frame = ok(&mut p.app, "surface", &json!({"popup": true}));
        assert_eq!((&frame["cols"], &frame["rows"]), (&json!(94), &json!(30)));
        let ran = p.popup_ended();
        assert_eq!((ran.context.as_str(), ran.ended.as_str(), ran.code), ("button", "exited", Some(0)));
        assert_eq!(ran.ids, shown);
        assert_eq!(Plugged::last_folder(&p.board), p.other_folder());
    }

    #[test]
    fn named_with_a_pane_opens_on_that_pane_sized_from_the_view() {
        let mut p = plugged();
        let (view, pane) = (p.opens_on_the_other_project((120, 40)), p.window_pane());

        p.starts(Some(view), "board", Some(json!({"pane": pane})));

        assert_eq!(p.popup_size(), (94, 30));
        let ran = p.popup_ended();
        assert_eq!((ran.context.as_str(), ran.ids.pane), ("pane", Some(pane)));
    }

    #[test]
    fn keeps_the_views_size_when_the_server_goes_on_with_the_size_a_window_left() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));

        p.app.resize(AREA);

        assert_eq!(p.popup_size(), (94, 30));
        p.popup_ended();
    }

    #[test]
    fn follows_the_view_when_the_view_gets_another_size() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));

        p.asks(Some(view), "view-set", &json!({"cols": 60, "rows": 20, "claim": false}));
        p.reply().expect("the new size");

        assert_eq!(p.popup_size(), (46, 14));
        p.popup_ended();
    }

    #[test]
    fn opens_at_its_own_size_in_a_view_without_one() {
        let mut p = plugged();
        let view = p.opens(&json!({}));

        p.opens_popup(Some(view));

        assert_eq!(p.popup_size(), (80, 24));
        p.popup_ended();
    }

    #[test]
    fn takes_the_size_of_a_window_while_one_is_attached() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.app.window_attached();
        p.app.resize(AREA);

        p.opens_popup(Some(view));

        assert_eq!(p.popup_size(), (78, 14));
        p.popup_ended();
    }

    #[test]
    fn goes_back_to_the_views_size_when_the_last_window_leaves() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));
        p.app.window_attached();
        p.app.resize(AREA);
        assert_eq!(p.popup_size(), (78, 14));
        p.asks(Some(view), "view-set", &json!({"cols": 60, "rows": 20, "claim": false}));
        p.reply().expect("the new size");
        assert_eq!(p.popup_size(), (78, 14), "the window sizes it while it is attached");

        p.app.window_gone();

        assert_eq!(p.popup_size(), (46, 14));
        p.popup_ended();
    }

    #[test]
    fn stays_open_at_its_size_when_its_view_closes_until_someone_closes_it() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));

        p.app.forget(VIEWER);
        p.app.resize(AREA);

        assert_eq!(p.popup_size(), (94, 30));
        assert!(p.app.app_state().popup.is_some());
        let client = CLIENT + 1;
        let close = json!({"contract": 1, "command": "plugin-popup-close", "args": {}}).to_string();
        p.app.request(client, &close, Some(AREA), Instant::now());
        let answers = p.app.take_answers();
        assert_eq!(answers.len(), 2, "{answers:?}");
        let ran: ActionRan =
            serde_json::from_value(reply_of(&answers[0].1).expect("how it ended")).expect("an outcome");
        assert_eq!((answers[0].0, ran.ended.as_str(), ran.code), (CLIENT, "closed", None));
        std::fs::remove_file(p.board.with_extension("hold")).expect("let the popup's program end");
    }

    #[test]
    fn is_sized_by_a_window_that_attaches_after_its_view_closed() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));
        p.app.forget(VIEWER);

        p.app.window_attached();
        p.app.resize(AREA);

        assert_eq!(p.popup_size(), (78, 14));
        p.popup_ended();
    }

    #[test]
    fn is_a_conflict_while_another_popup_is_open() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.opens_popup(Some(view));

        p.starts(Some(view), "board", None);
        let answers = p.app.take_answers();

        let refused = reply_of(&answers[0].1).expect_err("refused");
        assert_eq!((refused.code.as_str(), refused.message.as_str()), ("conflict", "a popup is already open"));
        p.popup_ended();
    }

    #[test]
    fn is_a_conflict_while_a_dialog_is_open() {
        let mut p = plugged();
        let view = p.opens_on_the_other_project((120, 40));
        p.app.open_settings();

        let refused = p.runs(Some(view), "board", None).expect_err("refused");

        assert_eq!((refused.code.as_str(), refused.message.as_str()), ("conflict", "a dialog is open in the window"));
        assert!(p.app.popup().is_none());
    }
}
