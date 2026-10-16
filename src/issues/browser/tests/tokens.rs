use super::*;

fn on_shortcut() -> Browser {
    let mut b = browser();
    b.tab_to(Tab::One(Source::Shortcut));
    b
}

#[test]
fn a_tracker_without_a_token_asks_for_one() {
    let b = on_shortcut();
    assert_eq!((b.token_form(), b.labels(ROOM.buttons)), (Some(Source::Shortcut), vec!["connect", "cancel"]));
}

#[test]
fn enter_checks_the_typed_token() {
    let mut b = on_shortcut();
    type_text(&mut b, "t0k");
    assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Shortcut, Secret("t0k".into())));
}

#[test]
fn the_token_is_never_shown() {
    let mut b = on_shortcut();
    type_text(&mut b, "t0k");
    assert_eq!(b.token_field(Source::Shortcut).1, "•••");
}

#[test]
fn an_empty_token_is_not_sent() {
    let mut b = on_shortcut();
    assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
}

#[test]
fn once_connected_the_tab_lists_and_offers_to_disconnect() {
    let mut b = on_shortcut();
    b.connected(Source::Shortcut, Connection { from_env: false, account: None });
    assert_eq!(
        (b.needs_load(), b.labels(ROOM.buttons)),
        (
            Action::Load(vec![Source::Shortcut]),
            vec!["start", "schedule", "start a flow", "refresh", "disconnect", "cancel"]
        )
    );
}

#[test]
fn a_token_from_the_environment_cannot_be_disconnected() {
    let mut b = on_shortcut();
    b.connected(Source::Shortcut, Connection { from_env: true, account: None });
    assert_eq!(b.labels(ROOM.buttons), ["start", "schedule", "start a flow", "refresh", "cancel"]);
}

fn field(b: &Browser) -> (&'static str, String) {
    b.token_field(b.token_form().expect("the token form"))
}

mod jira {
    use super::*;

    fn on_jira() -> Browser {
        let mut b = browser();
        b.jira_site = "acme.atlassian.net".into();
        b.jira_email = "ana@acme.dev".into();
        b.tab_to(Tab::One(Source::Jira));
        b
    }

    fn to_the_token(b: &mut Browser) -> Action {
        press(b, KeyCode::Enter);
        press(b, KeyCode::Enter)
    }

    #[test]
    fn asks_the_site_first_with_the_saved_one_typed() {
        let b = on_jira();
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("site", "acme.atlassian.net".into()), vec!["next", "cancel"])
        );
    }

    #[test]
    fn then_the_email_and_saves_both() {
        let mut b = on_jira();
        press(&mut b, KeyCode::Enter);
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("email", "ana@acme.dev".into()), vec!["next", "back", "cancel"])
        );
        let saved = press(&mut b, KeyCode::Enter);
        assert_eq!(saved, Action::SaveJira { site: "acme.atlassian.net".into(), email: "ana@acme.dev".into() });
    }

    #[test]
    fn a_site_typed_as_a_name_gets_its_domain() {
        let mut b = on_jira();
        b.jira_site.clear();
        type_text(&mut b, "shop");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.jira_site, "shop.atlassian.net");
    }

    #[test]
    fn a_bad_email_stays_with_its_error() {
        let mut b = on_jira();
        b.jira_email = "ana".into();
        let action = to_the_token(&mut b);
        let error = b.forms.get(&Source::Jira).and_then(|f| f.error.clone());
        assert_eq!(
            (action, field(&b).0, error.as_deref()),
            (Action::None, "email", Some("type the email of your Atlassian account"))
        );
    }

    #[test]
    fn the_token_comes_last_and_is_hidden() {
        let mut b = on_jira();
        to_the_token(&mut b);
        type_text(&mut b, "t0k");
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("API token", "•••".into()), vec!["connect", "back", "cancel"])
        );
        assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Jira, Secret("t0k".into())));
    }

    #[test]
    fn back_returns_to_the_email() {
        let mut b = on_jira();
        to_the_token(&mut b);
        let pos = button(&b, "back");
        click(&mut b, pos);
        assert_eq!(field(&b), ("email", "ana@acme.dev".into()));
    }
}

mod plane {
    use super::*;

    fn on_plane() -> Browser {
        let mut b = browser();
        b.plane_workspace = "acme".into();
        b.tab_to(Tab::One(Source::Plane));
        b
    }

    #[test]
    fn asks_the_workspace_first_with_the_saved_one_typed() {
        let b = on_plane();
        assert_eq!((field(&b), b.labels(ROOM.buttons)), (("workspace", "acme".into()), vec!["next", "cancel"]));
    }

    #[test]
    fn saves_the_workspace_and_then_asks_for_the_key() {
        let mut b = on_plane();
        assert_eq!(press(&mut b, KeyCode::Enter), Action::SavePlane { workspace: "acme".into() });
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("API key", String::new()), vec!["connect", "back", "cancel"])
        );
    }

    #[test]
    fn a_workspace_typed_in_capitals_is_saved_as_a_slug() {
        let mut b = on_plane();
        b.plane_workspace.clear();
        type_text(&mut b, " Acme-Team ");
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.plane_workspace, "acme-team");
    }

    #[test]
    fn a_bad_workspace_stays_with_its_error() {
        let mut b = on_plane();
        b.plane_workspace = "acme corp".into();
        let action = press(&mut b, KeyCode::Enter);
        let error = b.forms.get(&Source::Plane).and_then(|f| f.error.clone());
        let expected = "a workspace is lowercase letters, numbers and dashes, such as acme";
        assert_eq!((action, field(&b).0, error.as_deref()), (Action::None, "workspace", Some(expected)));
    }

    #[test]
    fn the_key_is_hidden_and_checked() {
        let mut b = on_plane();
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "k3y");
        assert_eq!(field(&b), ("API key", "•••".into()));
        assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Plane, Secret("k3y".into())));
    }

    #[test]
    fn back_returns_to_the_workspace() {
        let mut b = on_plane();
        press(&mut b, KeyCode::Enter);
        let pos = button(&b, "back");
        click(&mut b, pos);
        assert_eq!(field(&b), ("workspace", "acme".into()));
    }
}

mod asana {
    use super::*;

    fn on_asana() -> Browser {
        let mut b = browser();
        b.tab_to(Tab::One(Source::Asana));
        b
    }

    fn at_the_workspace_step() -> Browser {
        let mut b = on_asana();
        let workspaces = vec![("w-1".to_string(), "Acme".to_string()), ("w-2".to_string(), "Beta".to_string())];
        b.choose_workspace(Source::Asana, Secret("t0k".into()), workspaces);
        b
    }

    fn error(b: &Browser) -> Option<String> {
        b.forms.get(&Source::Asana).and_then(|f| f.error.clone())
    }

    #[test]
    fn asks_for_the_token_first() {
        let mut b = on_asana();
        assert_eq!((field(&b), b.labels(ROOM.buttons)), (("access token", String::new()), vec!["connect", "cancel"]));
        type_text(&mut b, "t0k");
        assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Asana, Secret("t0k".into())));
    }

    #[test]
    fn asks_which_workspace_once_the_token_is_known() {
        let b = at_the_workspace_step();
        assert_eq!((field(&b), b.labels(ROOM.buttons)), (("workspace", String::new()), vec!["next", "back", "cancel"]));
    }

    #[rstest::rstest]
    #[case::by_name("beta", "w-2")]
    #[case::ignoring_case_and_spaces("  ACME ", "w-1")]
    #[case::by_id("w-2", "w-2")]
    fn a_workspace_of_the_token_is_saved_with_the_token(#[case] typed: &str, #[case] saved: &str) {
        let mut b = at_the_workspace_step();
        type_text(&mut b, typed);
        assert_eq!(
            press(&mut b, KeyCode::Enter),
            Action::SaveAsana { workspace: saved.into(), token: Secret("t0k".into()) }
        );
    }

    #[rstest::rstest]
    #[case::another("gamma", "no Asana workspace named gamma; yours are Acme, Beta")]
    #[case::nothing("", "type one of your workspaces: Acme, Beta")]
    fn any_other_workspace_stays_with_its_error(#[case] typed: &str, #[case] message: &str) {
        let mut b = at_the_workspace_step();
        type_text(&mut b, typed);
        assert_eq!(press(&mut b, KeyCode::Enter), Action::None);
        assert_eq!((field(&b).0, error(&b).as_deref()), ("workspace", Some(message)));
    }

    #[test]
    fn back_returns_to_the_token() {
        let mut b = at_the_workspace_step();
        let pos = button(&b, "back");
        click(&mut b, pos);
        assert_eq!((field(&b), b.labels(ROOM.buttons)), (("access token", String::new()), vec!["connect", "cancel"]));
    }
}

mod gitlab {
    use super::*;

    fn on_gitlab() -> Browser {
        let mut b = browser();
        b.tab_to(Tab::One(Source::Gitlab));
        b
    }

    #[test]
    fn asks_the_server_first_with_the_saved_one_typed() {
        let mut b = on_gitlab();
        b.gitlab_url = "https://gitlab.acme.dev".into();
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("server", "https://gitlab.acme.dev".into()), vec!["next", "cancel"])
        );
    }

    #[test]
    fn an_empty_server_is_saved_and_then_the_token_is_asked() {
        let mut b = on_gitlab();
        assert_eq!(press(&mut b, KeyCode::Enter), Action::SaveGitlab { url: String::new() });
        assert_eq!(
            (field(&b), b.labels(ROOM.buttons)),
            (("access token", String::new()), vec!["connect", "back", "cancel"])
        );
    }

    #[test]
    fn a_typed_server_is_saved_without_its_last_slash() {
        let mut b = on_gitlab();
        type_text(&mut b, "https://gitlab.acme.dev/");
        assert_eq!(press(&mut b, KeyCode::Enter), Action::SaveGitlab { url: "https://gitlab.acme.dev".into() });
        assert_eq!(b.gitlab_url, "https://gitlab.acme.dev");
    }

    #[test]
    fn a_bad_server_stays_with_its_error() {
        let mut b = on_gitlab();
        type_text(&mut b, "gitlab.acme.dev");
        let action = press(&mut b, KeyCode::Enter);
        let error = b.forms.get(&Source::Gitlab).and_then(|f| f.error.clone());
        let expected = "a URL starts with https://, such as https://gitlab.acme.dev";
        assert_eq!((action, field(&b).0, error.as_deref()), (Action::None, "server", Some(expected)));
    }

    #[test]
    fn the_token_is_hidden_and_checked() {
        let mut b = on_gitlab();
        press(&mut b, KeyCode::Enter);
        type_text(&mut b, "glpat");
        assert_eq!(field(&b), ("access token", "•••••".into()));
        assert_eq!(press(&mut b, KeyCode::Enter), Action::CheckToken(Source::Gitlab, Secret("glpat".into())));
    }

    #[test]
    fn back_returns_to_the_server() {
        let mut b = on_gitlab();
        b.gitlab_url = "https://gitlab.acme.dev".into();
        press(&mut b, KeyCode::Enter);
        let pos = button(&b, "back");
        click(&mut b, pos);
        assert_eq!(field(&b), ("server", "https://gitlab.acme.dev".into()));
    }
}

#[test]
fn disconnect_asks_to_forget_the_token() {
    let mut b = on_shortcut();
    b.connected(Source::Shortcut, Connection { from_env: false, account: None });
    let pos = button(&b, "disconnect");
    assert_eq!(click(&mut b, pos), Action::Disconnect(Source::Shortcut));
}
