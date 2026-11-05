use super::*;
use crate::agents::activity::Status;

fn card(down: Option<Retry>) -> RemoteCard {
    RemoteCard {
        machine: "vps".into(),
        agent: Some(("claude".into(), Some(Status::Waiting))),
        folder: "~/code/web-shop".into(),
        down,
    }
}

fn shown(card: RemoteCard) -> Terminal<TestBackend> {
    let v = View { has_project: true, remote_project: true, remote_card: Some(card), ..view(&["web-shop"]) };
    render_sized(&v, W, H)
}

fn text(card: RemoteCard) -> String {
    let t = shown(card);
    (0..H).map(|y| row_text(&t, Rect::new(0, y, W, 1))).collect::<Vec<_>>().join("\n")
}

#[test]
fn renders_where_a_machine_s_workspace_runs() {
    insta::assert_snapshot!(shown(card(None)).backend());
}

#[test]
fn renders_a_fallen_machine_s_card() {
    insta::assert_snapshot!(shown(card(Some(Retry::In(4)))).backend());
}

#[rstest]
#[case::no_agent(None, None, &["runs on vps", "no agent there"])]
#[case::working(Some(Status::Working), None, &["claude is working"])]
#[case::failed(Some(Status::Failed), None, &["claude failed"])]
#[case::done(Some(Status::Done), Some(Retry::Now), &["vps unreachable", "reconnecting…", "claude keeps working there"])]
#[case::a_person_needed(Some(Status::Idle), Some(Retry::Because("not found".into())), &["not found"])]
fn says_what_its_agent_does(#[case] status: Option<Status>, #[case] down: Option<Retry>, #[case] lines: &[&str]) {
    let agent = status.map(|status| ("claude".to_string(), Some(status)));
    let shown = text(RemoteCard { agent, ..card(down) });

    assert!(lines.iter().all(|line| shown.contains(line)), "{shown}");
}

#[test]
fn a_fallen_machine_without_an_agent_there_says_its_shells_keep_running() {
    let shown = text(RemoteCard { agent: None, ..card(Some(Retry::In(1))) });

    assert!(shown.contains("its shells keep running there") && shown.contains("reconnecting in 1 s"), "{shown}");
}

#[test]
fn a_long_folder_is_cut_at_its_start() {
    let folder = format!("~/{}web-shop", "deep/".repeat(20));

    let shown = text(RemoteCard { folder, ..card(None) });

    assert!(shown.contains("…") && shown.contains("deep/web-shop") && !shown.contains("~/deep"), "{shown}");
}
