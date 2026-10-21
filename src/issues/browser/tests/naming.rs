use super::*;

fn naming() -> Browser {
    let mut b = with_lists();
    press(&mut b, KeyCode::Down);
    press(&mut b, KeyCode::Enter);
    let Action::Name(issue, kind, place) = press(&mut b, KeyCode::Enter) else { panic!("asks for the names") };
    let name = format!("{} {}", issue.key, issue.title);
    b.name_start(Naming {
        issue,
        kind,
        place,
        branch: "issue-1-old-bug".into(),
        name: name.clone(),
        default_name: name,
        field: Field::Branch,
    });
    b
}

fn started(action: Action) -> (String, String) {
    let Action::StartNamed(naming) = action else { panic!("starts, not {action:?}") };
    (naming.branch, naming.name)
}

#[test]
fn enter_starts_with_what_was_filled_in() {
    let mut b = naming();
    assert_eq!(b.labels(ROOM.buttons), ["start", "back"]);
    assert_eq!(started(press(&mut b, KeyCode::Enter)), ("issue-1-old-bug".into(), "#1 old bug".into()));
}

#[test]
fn both_fields_can_be_edited() {
    let mut b = naming();
    for _ in 0.."issue-1-old-bug".len() {
        press(&mut b, KeyCode::Backspace);
    }
    type_text(&mut b, "feat/1");
    press(&mut b, KeyCode::Tab);
    for _ in 0.."#1 old bug".len() {
        press(&mut b, KeyCode::Backspace);
    }
    b.paste("Old bug\n");
    assert_eq!(started(press(&mut b, KeyCode::Enter)), ("feat/1".into(), "Old bug".into()));
}

#[test]
fn an_empty_name_keeps_the_issue_s_own() {
    let mut b = naming();
    click(&mut b, IssuesHit::Field(1));
    for _ in 0..20 {
        press(&mut b, KeyCode::Backspace);
    }
    assert_eq!(started(press(&mut b, KeyCode::Enter)).1, "#1 old bug");
}

#[test]
fn a_branch_git_would_refuse_is_not_started() {
    let mut b = naming();
    type_text(&mut b, " x");
    assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
    assert!(b.error.as_deref().is_some_and(|e| e.contains("cannot be a branch")), "{:?}", b.error);
    press(&mut b, KeyCode::Backspace);
    assert!(b.error.is_none());
}

#[test]
fn esc_and_back_go_back_to_the_issue() {
    let mut b = naming();
    press(&mut b, KeyCode::Esc);
    assert!(b.naming.is_none());
    assert!(matches!(b.screen, Screen::Detail { .. }));
    let mut b = naming();
    let back = button(&b, "back");
    assert_eq!(click(&mut b, back), Action::None);
    assert!(b.naming.is_none());
}

#[test]
fn the_start_button_starts_too() {
    let mut b = naming();
    let start = button(&b, "start");
    assert_eq!(started(click(&mut b, start)).0, "issue-1-old-bug");
}

#[test]
fn a_new_tab_needs_no_names() {
    let mut b = Browser { worktrees: false, ..with_lists() };
    press(&mut b, KeyCode::Down);
    press(&mut b, KeyCode::Enter);
    assert!(matches!(press(&mut b, KeyCode::Enter), Action::Start(..)));
}
