use super::*;

fn edit(s: &mut Settings, row: &Row, text: &str) -> Action {
    go_to(s, row);
    press(s, KeyCode::Enter);
    s.edit.iter_mut().for_each(|e| e.input.clear());
    type_text(s, text);
    press(s, KeyCode::Enter)
}

#[test]
fn a_pasted_token_is_checked() {
    let mut s = settings();
    go_to(&mut s, &Row::Token(Source::Shortcut));
    press(&mut s, KeyCode::Enter);
    s.paste("t0k\n");
    assert_eq!(press(&mut s, KeyCode::Enter), Action::CheckToken(Source::Shortcut, Secret("t0k".into())));
}

#[test]
fn a_good_one_shows_its_account() {
    let mut s = settings();
    go_to(&mut s, &Row::Token(Source::Shortcut));
    press(&mut s, KeyCode::Enter);
    s.checking.push(Source::Shortcut);
    let account = Account { handle: "ana".into(), workspace: "acme".into() };
    s.checked(Source::Shortcut, Ok(account.clone()));
    assert_eq!((s.edit.is_none(), s.status(Source::Shortcut)), (true, Some(&Status::Saved(Some(account)))));
}

#[test]
fn a_rejected_one_stays_in_the_field() {
    let mut s = settings();
    go_to(&mut s, &Row::Token(Source::Shortcut));
    press(&mut s, KeyCode::Enter);
    s.checked(Source::Shortcut, Err("Shortcut rejected the token".into()));
    assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("Shortcut rejected the token"));
}

#[test]
fn one_from_the_environment_cannot_be_typed() {
    let mut s = settings();
    go_to(&mut s, &Row::Token(Source::Linear));
    press(&mut s, KeyCode::Enter);
    assert!(s.edit.is_none());
}

#[test]
fn a_saved_one_can_be_removed_from_its_row() {
    let mut s = settings();
    s.tokens[0].1 = Status::Saved(None);
    go_to(&mut s, &Row::Token(Source::Shortcut));
    assert_eq!(press(&mut s, KeyCode::Delete), Action::RemoveToken(Source::Shortcut));
}

mod jira {
    use super::*;

    fn with_jira() -> Settings {
        let mut s = settings();
        s.tokens.push((Source::Jira, Status::Missing));
        s
    }

    #[test]
    fn has_its_own_section_after_the_accounts() {
        let mut s = with_jira();
        s.open_page(Page::Issues);
        let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(6).collect();
        assert_eq!(
            rows,
            [
                ("Accounts", Row::Token(Source::Shortcut)),
                ("Accounts", Row::Token(Source::Linear)),
                ("Jira", Row::JiraSite),
                ("Jira", Row::JiraEmail),
                ("Jira", Row::Token(Source::Jira)),
                ("Jira", Row::JiraJql)
            ]
        );
    }

    #[test]
    fn the_site_is_saved_as_a_host_name() {
        let mut s = with_jira();
        let config = saved(edit(&mut s, &Row::JiraSite, "https://acme.atlassian.net/jira"));
        assert_eq!(config.jira_site, "acme.atlassian.net");
    }

    #[test]
    fn a_bad_email_stays_in_the_field() {
        let mut s = with_jira();
        assert_eq!(edit(&mut s, &Row::JiraEmail, "ana"), Action::None);
        assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("type the email of your Atlassian account"));
    }

    #[test]
    fn the_filter_is_saved_as_typed() {
        let mut s = with_jira();
        assert_eq!(saved(edit(&mut s, &Row::JiraJql, " project = SHOP ")).jira_jql, "project = SHOP");
    }

    #[test]
    fn the_token_needs_the_site_and_the_email_first() {
        let mut s = with_jira();
        assert_eq!(edit(&mut s, &Row::Token(Source::Jira), "t0k"), Action::None);
        assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("set the Jira site and email first"));
    }

    #[test]
    fn the_token_is_checked_once_both_are_set() {
        let mut s = with_jira();
        s.config.jira_site = "acme.atlassian.net".into();
        s.config.jira_email = "ana@acme.dev".into();
        let action = edit(&mut s, &Row::Token(Source::Jira), "t0k");
        assert_eq!(action, Action::CheckToken(Source::Jira, Secret("t0k".into())));
    }
}

mod plane {
    use super::*;

    fn with_plane() -> Settings {
        let mut s = settings();
        s.tokens.push((Source::Plane, Status::Missing));
        s
    }

    #[test]
    fn has_its_own_section_after_the_accounts() {
        let mut s = with_plane();
        s.open_page(Page::Issues);
        let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(6).collect();
        assert_eq!(
            rows,
            [
                ("Accounts", Row::Token(Source::Shortcut)),
                ("Accounts", Row::Token(Source::Linear)),
                ("Plane", Row::PlaneWorkspace),
                ("Plane", Row::PlaneUrl),
                ("Plane", Row::Token(Source::Plane)),
                ("Plane", Row::PlaneFilter)
            ]
        );
    }

    #[test]
    fn the_workspace_is_saved_as_a_slug() {
        let mut s = with_plane();
        assert_eq!(saved(edit(&mut s, &Row::PlaneWorkspace, " /Acme/ ")).plane_workspace, "acme");
    }

    #[test]
    fn a_bad_workspace_stays_in_the_field() {
        let mut s = with_plane();
        assert_eq!(edit(&mut s, &Row::PlaneWorkspace, "acme corp"), Action::None);
        let error = s.edit.and_then(|e| e.error);
        let expected = "a workspace is lowercase letters, numbers and dashes, such as acme";
        assert_eq!(error.as_deref(), Some(expected));
    }

    #[test]
    fn the_url_is_saved_without_its_trailing_slash() {
        let mut s = with_plane();
        let config = saved(edit(&mut s, &Row::PlaneUrl, "https://plane.acme.dev/"));
        assert_eq!(config.plane_url, "https://plane.acme.dev");
    }

    #[test]
    fn a_bad_url_stays_in_the_field() {
        let mut s = with_plane();
        assert_eq!(edit(&mut s, &Row::PlaneUrl, "plane.acme.dev"), Action::None);
        let error = s.edit.and_then(|e| e.error);
        assert_eq!(error.as_deref(), Some("a URL starts with https://, such as https://plane.acme.dev"));
    }

    #[test]
    fn an_empty_url_goes_back_to_plane_cloud() {
        let mut s = with_plane();
        s.config.plane_url = "https://plane.acme.dev".into();
        assert_eq!(saved(edit(&mut s, &Row::PlaneUrl, "")).plane_url, "");
    }

    #[test]
    fn the_filter_is_saved_as_typed() {
        let mut s = with_plane();
        assert_eq!(saved(edit(&mut s, &Row::PlaneFilter, " priority=high ")).plane_filter, "priority=high");
    }

    #[test]
    fn the_key_needs_the_workspace_first() {
        let mut s = with_plane();
        assert_eq!(edit(&mut s, &Row::Token(Source::Plane), "k3y"), Action::None);
        assert_eq!(s.edit.and_then(|e| e.error).as_deref(), Some("set the Plane workspace first"));
    }

    #[test]
    fn the_key_is_checked_once_the_workspace_is_set() {
        let mut s = with_plane();
        s.config.plane_workspace = "acme".into();
        let action = edit(&mut s, &Row::Token(Source::Plane), "k3y");
        assert_eq!(action, Action::CheckToken(Source::Plane, Secret("k3y".into())));
    }
}

mod asana {
    use super::*;

    fn with_asana() -> Settings {
        let mut s = settings();
        s.tokens.push((Source::Asana, Status::Missing));
        s
    }

    #[test]
    fn has_its_own_section_after_the_accounts() {
        let mut s = with_asana();
        s.open_page(Page::Issues);
        let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(5).collect();
        assert_eq!(
            rows,
            [
                ("Accounts", Row::Token(Source::Shortcut)),
                ("Accounts", Row::Token(Source::Linear)),
                ("Asana", Row::AsanaWorkspace),
                ("Asana", Row::AsanaProjects),
                ("Asana", Row::Token(Source::Asana))
            ]
        );
    }

    #[test]
    fn the_rows_say_what_empty_means() {
        let s = with_asana();
        let workspace = s.row_text(&Row::AsanaWorkspace);
        let projects = s.row_text(&Row::AsanaProjects);
        assert_eq!(
            [&workspace.label, &workspace.value, &workspace.note],
            ["Asana workspace", "your only one", "name or ID"]
        );
        assert_eq!(
            [&projects.label, &projects.value, &projects.note],
            ["Asana projects", "your tasks", "IDs or links, empty lists your tasks"]
        );
    }

    #[test]
    fn the_workspace_is_saved_as_typed_without_the_spaces_around() {
        let mut s = with_asana();
        assert_eq!(saved(edit(&mut s, &Row::AsanaWorkspace, "  Acme Corp ")).asana_workspace, "Acme Corp");
        assert_eq!(s.row_text(&Row::AsanaWorkspace).value, "Acme Corp");
    }

    #[test]
    fn an_empty_workspace_goes_back_to_the_only_one() {
        let mut s = with_asana();
        s.config.asana_workspace = "Acme".into();
        assert_eq!(saved(edit(&mut s, &Row::AsanaWorkspace, "")).asana_workspace, "");
        assert_eq!(s.notice.as_deref(), Some("the Asana workspace was cleared"));
    }

    #[test]
    fn projects_are_saved_as_ids_whatever_way_they_were_typed() {
        let mut s = with_asana();
        let typed = "https://app.asana.com/0/123/list, 456  https://app.asana.com/1/9/project/789/board";
        assert_eq!(saved(edit(&mut s, &Row::AsanaProjects, typed)).asana_projects, "123, 456, 789");
        assert_eq!(s.notice.as_deref(), Some("Asana projects: 123, 456, 789"));
    }

    #[test]
    fn no_projects_list_your_tasks() {
        let mut s = with_asana();
        s.config.asana_projects = "123".into();
        assert_eq!(saved(edit(&mut s, &Row::AsanaProjects, " ")).asana_projects, "");
        assert_eq!(s.notice.as_deref(), Some("Asana lists your tasks"));
    }

    #[test]
    fn something_that_is_no_project_stays_in_the_field() {
        let mut s = with_asana();
        assert_eq!(edit(&mut s, &Row::AsanaProjects, "123 web shop"), Action::None);
        let error = s.edit.and_then(|e| e.error);
        let expected =
            "type project IDs or links, such as 1200000000000001 or https://app.asana.com/0/1200000000000001/list";
        assert_eq!(error.as_deref(), Some(expected));
    }

    #[test]
    fn the_token_does_not_need_the_workspace_first() {
        let mut s = with_asana();
        let action = edit(&mut s, &Row::Token(Source::Asana), "t0k");
        assert_eq!(action, Action::CheckToken(Source::Asana, Secret("t0k".into())));
    }
}

mod gitlab {
    use super::*;

    fn with_gitlab() -> Settings {
        let mut s = settings();
        s.tokens.push((Source::Gitlab, Status::Missing));
        s
    }

    #[test]
    fn has_its_own_section_after_the_accounts() {
        let mut s = with_gitlab();
        s.open_page(Page::Issues);
        let rows: Vec<(&str, Row)> = s.rows().into_iter().map(|r| (r.section(), r)).take(4).collect();
        assert_eq!(
            rows,
            [
                ("Accounts", Row::Token(Source::Shortcut)),
                ("Accounts", Row::Token(Source::Linear)),
                ("GitLab", Row::GitlabUrl),
                ("GitLab", Row::Token(Source::Gitlab))
            ]
        );
    }

    #[test]
    fn an_empty_url_is_gitlab_com() {
        let s = with_gitlab();
        let row = s.row_text(&Row::GitlabUrl);
        assert_eq!((row.label.as_str(), row.value.as_str()), ("GitLab URL", "gitlab.com"));
    }

    #[test]
    fn the_url_is_saved_without_its_trailing_slash() {
        let mut s = with_gitlab();
        let config = saved(edit(&mut s, &Row::GitlabUrl, "https://gitlab.acme.dev/"));
        assert_eq!(
            (config.gitlab_url.as_str(), s.notice.as_deref()),
            ("https://gitlab.acme.dev", Some("GitLab server: https://gitlab.acme.dev"))
        );
    }

    #[test]
    fn a_url_without_encryption_says_so() {
        let mut s = with_gitlab();
        saved(edit(&mut s, &Row::GitlabUrl, "http://gitlab.acme.dev"));
        let expected = "GitLab server: http://gitlab.acme.dev; the address starts with http://, so the token \
                        travels unencrypted";
        assert_eq!(s.notice.as_deref(), Some(expected));
    }

    #[test]
    fn a_bad_url_stays_in_the_field() {
        let mut s = with_gitlab();
        assert_eq!(edit(&mut s, &Row::GitlabUrl, "gitlab.acme.dev"), Action::None);
        let error = s.edit.and_then(|e| e.error);
        assert_eq!(error.as_deref(), Some("a URL starts with https://, such as https://gitlab.acme.dev"));
    }

    #[test]
    fn an_empty_url_goes_back_to_gitlab_com() {
        let mut s = with_gitlab();
        s.config.gitlab_url = "https://gitlab.acme.dev".into();
        assert_eq!(saved(edit(&mut s, &Row::GitlabUrl, "")).gitlab_url, "");
        assert_eq!(s.notice.as_deref(), Some("GitLab is gitlab.com"));
    }

    #[test]
    fn the_token_is_checked_without_anything_else_set() {
        let mut s = with_gitlab();
        let action = edit(&mut s, &Row::Token(Source::Gitlab), "glpat");
        assert_eq!(action, Action::CheckToken(Source::Gitlab, Secret("glpat".into())));
    }
}

#[test]
fn are_never_shown() {
    let mut s = settings();
    go_to(&mut s, &Row::Token(Source::Shortcut));
    press(&mut s, KeyCode::Enter);
    type_text(&mut s, "t0k");
    assert_eq!(s.edit_field().map(|(_, value)| value).as_deref(), Some("•••"));
}
