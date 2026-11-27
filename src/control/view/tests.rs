use rstest::rstest;

use super::*;
use crate::control::contract::{FEATURES, VIEW_VERSION};
use crate::control::{Request, Status};

#[rstest]
#[case::none(None, None, Ok(None))]
#[case::both(Some(100), Some(30), Ok(Some((100, 30))))]
#[case::half(Some(100), None, Err("both"))]
#[case::too_wide(Some(1001), Some(30), Err("cols"))]
#[case::too_tall(Some(100), Some(501), Err("rows"))]
#[case::zero(Some(0), Some(30), Err("cols"))]
fn a_size_is_both_or_neither(
    #[case] cols: Option<u16>,
    #[case] rows: Option<u16>,
    #[case] expected: Result<Option<(u16, u16)>, &str>,
) {
    let got = size(cols, rows);

    match expected {
        Ok(size) => assert_eq!(got, Ok(size)),
        Err(word) => assert!(got.expect_err("refused").contains(word)),
    }
}

#[test]
fn the_view_in_a_request_is_left_out_when_absent() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        bot: None,
        bot_run: None,
        command: Command::Status(Status {}),
    };

    let text = serde_json::to_string(&request).expect("json");

    assert!(!text.contains("view"), "{text}");
    assert!(serde_json::to_string(&Request { view: Some(4), ..request }).expect("json").contains("\"view\":4"));
}

#[test]
fn the_commands_that_take_a_view_are_the_ones_that_act_on_a_place() {
    assert!(takes_view(&Command::Status(Status {})));
    assert!(takes_view(&Command::ViewSet(SetView::default())));
    assert!(takes_view(&Command::Go(crate::control::Go::default())));
    assert!(takes_view(&Command::Clone(crate::control::CloneRepository::default())));
    let run = crate::control::PluginRun { plugin: "a".into(), action: "b".into(), revision: 0, item: None };
    assert!(takes_view(&Command::PluginRun(run)));
    assert!(!takes_view(&Command::PluginActions(crate::control::PluginActions::default())));
    assert!(!takes_view(&Command::Stats(crate::control::Stats::default())));
    assert!(!takes_view(&Command::View(OpenView::default())));
}

#[test]
fn a_flow_run_takes_a_view_and_the_other_flow_requests_do_not() {
    assert!(takes_view(&Command::FlowRun(crate::control::FlowRun::default())));
    assert!(!takes_view(&Command::FlowList(crate::control::FlowList::default())));
}

#[test]
fn views_are_a_feature_and_a_stream_of_the_contract() {
    assert!(FEATURES.contains(&"views"));
    assert!(FEATURES.contains(&"plugin-run-view"));
    assert!(FEATURES.contains(&"flow-run-view"));
    assert_eq!(VIEW_VERSION, 1);
}

#[test]
fn a_view_that_is_skipped_by_a_reader_reads_back_whole() {
    let info = ViewInfo {
        id: 41,
        projection: 3,
        active: true,
        cols: Some(120),
        rows: Some(40),
        shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
        controls: true,
        visible: false,
        claims: false,
        bot: Some("prs".into()),
    };

    let written = serde_json::to_string_pretty(&info).expect("json");

    assert_eq!(serde_json::from_str::<ViewInfo>(&written).expect("a view"), info);
}

fn text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("json")
}

#[test]
fn requests_and_views_leave_out_a_bot_they_do_not_name() {
    use crate::control::{OpenView, SetView};

    let named = Some("prs".to_string());
    for none in [text(&OpenView::default()), text(&SetView::default()), text(&ViewInfo::default())] {
        assert!(!none.contains("\"bot\""), "{none}");
    }
    for some in [
        text(&OpenView { bot: named.clone(), ..OpenView::default() }),
        text(&SetView { bot: named.clone(), ..SetView::default() }),
        text(&ViewInfo { bot: named, ..ViewInfo::default() }),
    ] {
        assert!(some.contains("\"bot\":\"prs\""), "{some}");
    }
    assert!(text(&SetView { bot: Some(String::new()), ..SetView::default() }).contains("\"bot\":\"\""));
}

#[test]
fn requests_leave_out_no_claim_unless_they_ask_for_it() {
    use crate::control::{OpenView, SetView};

    for none in [text(&OpenView::default()), text(&SetView::default())] {
        assert!(!none.contains("no_claim"), "{none}");
    }
    for some in [
        text(&OpenView { no_claim: true, ..OpenView::default() }),
        text(&SetView { no_claim: true, ..SetView::default() }),
    ] {
        assert!(some.contains("\"no_claim\":true"), "{some}");
    }
}

#[test]
fn a_view_read_without_claims_claims() {
    let older: ViewInfo = serde_json::from_str(r#"{"id": 4, "active": true}"#).expect("a view");
    let watching: ViewInfo = serde_json::from_str(r#"{"id": 4, "claims": false}"#).expect("a view");

    assert!(older.claims);
    assert!(!watching.claims);
    assert!(text(&ViewInfo::default()).contains("\"claims\":false"));
}

#[rstest]
#[case::neither(false, false, Ok(None))]
#[case::claim(true, false, Ok(Some(true)))]
#[case::no_claim(false, true, Ok(Some(false)))]
#[case::both_are_refused(true, true, Err("`view-set` takes `claim` or `no_claim`, not both".to_string()))]
fn a_view_set_claims_releases_or_leaves_the_claim(
    #[case] claim: bool,
    #[case] no_claim: bool,
    #[case] expected: Result<Option<bool>, String>,
) {
    let set = SetView { claim, no_claim, ..SetView::default() };

    assert_eq!(set.claiming(), expected);
}

#[test]
fn a_view_read_without_visible_is_visible() {
    let older: ViewInfo = serde_json::from_str(r#"{"id": 4, "active": true}"#).expect("a view");
    let hidden: ViewInfo = serde_json::from_str(r#"{"id": 4, "visible": false}"#).expect("a view");

    assert!(older.visible);
    assert!(!hidden.visible);
}

#[test]
fn account_add_and_account_login_take_a_view_and_the_other_account_requests_do_not() {
    use crate::control::{AccountAdd, AccountList, AccountLogin, AccountRestart, AccountUse};

    assert!(takes_view(&Command::AccountAdd(AccountAdd::default())));
    assert!(takes_view(&Command::AccountLoginTab(AccountLogin::default())));
    assert!(!takes_view(&Command::AccountList(AccountList::default())));
    assert!(!takes_view(&Command::AccountUse(AccountUse::default())));
    assert!(!takes_view(&Command::AccountRestart(AccountRestart::default())));
    assert!(FEATURES.contains(&"account-login-tab"));
}
