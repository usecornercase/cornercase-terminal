use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;
use crate::config::Config;
use crate::settings::{Action, Page};
use crate::test_util::{TempDir, write_executable};

fn settings() -> Settings {
    let mut s = Settings::new(Config::default(), Some(std::path::PathBuf::from("/home/ana")), Vec::new(), Page::Ui);
    s.shells = ShellsHere {
        default: "/bin/zsh".into(),
        found: vec![
            Found { value: "/bin/zsh".into(), note: "zsh".into() },
            Found { value: "/bin/fish".into(), note: "fish".into() },
        ],
        problem: None,
    };
    s
}

fn go_to(s: &mut Settings, row: &Row) {
    s.cursor = s.rows().iter().position(|r| r == row).expect("the row is there");
}

fn press(s: &mut Settings, code: KeyCode) -> Action {
    s.key(KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }, 20)
}

fn type_text(s: &mut Settings, text: &str) {
    for c in text.chars() {
        press(s, KeyCode::Char(c));
    }
}

fn saved(action: Action) -> Config {
    match action {
        Action::Save(config) => *config,
        other => panic!("nothing saved: {other:?}"),
    }
}

fn program(dir: &TempDir, name: &str) -> String {
    let path = dir.path().join(name);
    write_executable(&path, "#!/bin/sh\n");
    path.display().to_string()
}

fn choose(s: &mut Settings, value: &str) -> Action {
    let at = s.pick_choices().iter().position(|item| item.value == value).expect("offered");
    s.choose(Some(at))
}

#[test]
fn both_rows_come_right_after_the_inactive_panes() {
    let s = settings();
    let rows = s.rows();
    let at = rows.iter().position(|row| *row == Row::DimPanes).expect("inactive panes");
    assert_eq!(rows[at + 1..at + 3], [Row::Shell, Row::ShellArgs]);
}

#[test]
fn the_default_shows_by_its_name() {
    let s = settings();
    let text = s.row_text(&Row::Shell);
    assert_eq!((text.label.as_str(), text.value.as_str()), ("shell", "default (zsh)"));
    assert!(!text.dangerous);
    assert_eq!(s.row_text(&Row::ShellArgs).value, "none");
}

#[test]
fn a_program_that_cannot_start_turns_the_note_red() {
    let mut s = settings();
    s.config.shell.program = "/nonexistent/fish".into();
    s.shells.problem = Some("there is no program there".into());
    let text = s.row_text(&Row::Shell);
    assert!(text.dangerous);
    assert_eq!(text.note, "cannot start: there is no program there; new panes start zsh");
}

#[test]
fn the_pick_offers_the_default_the_shells_found_and_any_other_program() {
    let mut s = settings();
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);
    let offered: Vec<String> = s.pick_choices().iter().map(|item| item.value.clone()).collect();
    assert_eq!(offered, ["default", "/bin/zsh", "/bin/fish", "another program…"]);
    assert_eq!(s.pick_title(), "Which shell should new panes start?");
    assert_eq!(s.pick_selected(), Some(0));
}

#[test]
fn choosing_a_shell_saves_it_and_drops_the_arguments_of_the_one_before() {
    let mut s = settings();
    s.config.shell = ShellChoice { program: "/bin/zsh".into(), args: vec!["-l".into()] };
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);

    let config = saved(choose(&mut s, "/bin/fish"));

    assert_eq!(config.shell, ShellChoice { program: "/bin/fish".into(), args: Vec::new() });
}

#[test]
fn choosing_the_shell_already_set_keeps_its_arguments() {
    let mut s = settings();
    s.config.shell = ShellChoice { program: "/bin/zsh".into(), args: vec!["-l".into()] };
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);

    assert_eq!(saved(choose(&mut s, "/bin/zsh")).shell.args, ["-l"]);
}

#[test]
fn choosing_the_default_clears_the_shell() {
    let mut s = settings();
    s.config.shell = ShellChoice { program: "/bin/fish".into(), args: vec!["-l".into()] };
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);

    let config = saved(choose(&mut s, "default"));

    assert_eq!(config.shell, ShellChoice::default());
    assert_eq!(s.notice.as_deref(), Some("new panes start your default shell, zsh"));
}

#[test]
fn another_program_opens_the_field_with_the_current_one() {
    let mut s = settings();
    s.config.shell.program = "/opt/nu".into();
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);

    assert!(matches!(choose(&mut s, "another program…"), Action::None));

    assert_eq!(s.edit_field().map(|(_, value)| value), Some("/opt/nu".to_string()));
}

#[cfg(unix)]
#[test]
fn a_typed_program_is_saved_as_written_when_it_can_start() {
    let dir = TempDir::new();
    let nu = program(&dir, "nu");
    let mut s = settings();
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);
    choose(&mut s, "another program…");
    type_text(&mut s, &nu);

    let config = saved(press(&mut s, KeyCode::Enter));

    assert_eq!(config.shell.program, nu);
}

#[test]
fn a_typed_program_that_cannot_start_shows_why_and_saves_nothing() {
    let mut s = settings();
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);
    choose(&mut s, "another program…");
    type_text(&mut s, "bin/nu");

    assert!(matches!(press(&mut s, KeyCode::Enter), Action::None));

    let error = s.edit.as_ref().and_then(|edit| edit.error.clone()).expect("an error under the field");
    assert!(error.starts_with("`bin/nu`: "), "{error}");
    assert_eq!(s.config.shell, ShellChoice::default());
}

#[test]
fn typing_default_clears_the_shell() {
    let mut s = settings();
    s.config.shell.program = "/bin/fish".into();
    go_to(&mut s, &Row::Shell);
    press(&mut s, KeyCode::Enter);
    choose(&mut s, "another program…");
    for _ in 0..20 {
        press(&mut s, KeyCode::Backspace);
    }
    type_text(&mut s, "Default");

    assert_eq!(saved(press(&mut s, KeyCode::Enter)).shell, ShellChoice::default());
}

#[test]
fn the_arguments_are_split_like_those_of_an_agent_and_keep_the_program() {
    let mut s = settings();
    s.config.shell.program = "/bin/fish".into();
    go_to(&mut s, &Row::ShellArgs);
    press(&mut s, KeyCode::Enter);
    type_text(&mut s, "-l 'two words'");

    let config = saved(press(&mut s, KeyCode::Enter));

    assert_eq!(config.shell, ShellChoice { program: "/bin/fish".into(), args: vec!["-l".into(), "two words".into()] });
    assert_eq!(s.row_text(&Row::ShellArgs).value, "-l 'two words'");
}

#[test]
fn the_arguments_alone_make_the_default_shell_start_with_them() {
    let mut s = settings();
    go_to(&mut s, &Row::ShellArgs);
    press(&mut s, KeyCode::Enter);
    type_text(&mut s, "--login");

    let config = saved(press(&mut s, KeyCode::Enter));

    assert_eq!(config.shell, ShellChoice { program: String::new(), args: vec!["--login".into()] });
    assert_eq!(s.notice.as_deref(), Some("zsh starts with: --login"));
}
