use rstest::rstest;

use super::*;
use crate::issues::branch;
use crate::test_util::{FakeHttp, decoded, redirected};

const ME: &str =
    r#"{"data":{"gid":"u-me","name":"Ana Pérez","email":"ana@acme.dev","workspaces":[{"gid":"w-1","name":"Acme"}]}}"#;
const ME_TWO: &str = r#"{"data":{"gid":"u-me","name":"Ana Pérez","email":"ana@acme.dev",
        "workspaces":[{"gid":"w-1","name":"Acme"},{"gid":"w-2","name":"Beta"}]}}"#;
const USERS: &str = r#"{"data":[{"gid":"u-2","name":"Luis","email":"luis@acme.dev"},
        {"gid":"u-me","name":"Ana Pérez","email":"ana@acme.dev"},{"gid":"u-3","name":"","email":""}],"next_page":null}"#;
const TASK: &str = r#"{"gid":"1200000000000042","name":"Login redirect loops","completed":false,
        "modified_at":"2026-09-19T14:00:00.000Z","created_at":"2026-09-01T10:00:00.000Z",
        "permalink_url":"https://app.asana.com/1/1/project/9/task/1200000000000042","resource_subtype":"default_task",
        "assignee":{"gid":"u-2","name":"Luis"},"created_by":{"gid":"u-me"},"tags":[{"name":"bug"},{"name":"p1"}],
        "memberships":[{"section":{"name":"In Progress"},"project":{"name":"Web"}}],
        "custom_fields":[{"representation_type":"text","display_value":"nope"},
                         {"representation_type":"custom_id","display_value":"ENG-42"}]}"#;

type Route = (&'static str, u16, String);

fn api(server: &FakeHttp, workspace: &str, projects: &[&str]) -> Api {
    Api {
        base: server.url(),
        token: "t0k".into(),
        workspace: workspace.into(),
        projects: projects.iter().map(ToString::to_string).collect(),
    }
}

fn tasks(rows: &[&str], next: Option<&str>) -> String {
    let next = next.map_or("null".to_string(), |offset| format!(r#"{{"offset":"{offset}"}}"#));
    format!(r#"{{"data":[{}],"next_page":{next}}}"#, rows.join(","))
}

fn task(gid: &str, extra: &str) -> String {
    format!(
        r#"{{"gid":"{gid}","name":"Task {gid}","completed":false,"modified_at":"2026-09-01T10:00:00.000Z",
        "resource_subtype":"default_task","assignee":null,"created_by":{{"gid":"u-me"}}{extra}}}"#
    )
}

fn listed(query: &Query, projects: &[&str], pages: Vec<Route>) -> (Listed, FakeHttp) {
    let mut routes =
        vec![("GET /users/me", 200, ME.to_string()), ("GET /workspaces/w-1/users", 200, USERS.to_string())];
    routes.extend(pages);
    let server = FakeHttp::start(routes);
    let listed = api(&server, "", projects).list(query).expect("list");
    (listed, server)
}

fn mine(body: String) -> Vec<Route> {
    vec![("GET /tasks", 200, body)]
}

fn titles(listed: &Listed) -> Vec<&str> {
    listed.issues.iter().map(|i| i.title.as_str()).collect()
}

#[test]
fn whoami_sends_the_token_as_a_bearer_and_names_the_workspace() {
    let server = FakeHttp::start(vec![("GET /users/me", 200, ME)]);

    let account = api(&server, "", &[]).whoami().expect("whoami");

    assert_eq!(account, Account { handle: "Ana Pérez".into(), workspace: "Acme".into() });
    assert!(server.request(0).to_lowercase().contains("authorization: bearer t0k"), "{}", server.request(0));
    assert_eq!(server.requests().len(), 1);
}

#[rstest]
#[case::the_only_one("", ME, "Acme")]
#[case::by_gid("w-2", ME_TWO, "Beta")]
#[case::by_name_ignoring_case("acme", ME_TWO, "Acme")]
#[case::with_spaces_around(" Beta ", ME_TWO, "Beta")]
fn the_workspace_is_the_setting_or_the_only_one(#[case] setting: &str, #[case] me: &'static str, #[case] name: &str) {
    let server = FakeHttp::start(vec![("GET /users/me", 200, me)]);
    let account = api(&server, setting, &[]).whoami().expect("whoami");
    assert_eq!(account.workspace, name);
}

#[rstest]
#[case::several_and_none_set(
    "",
    ME_TWO,
    "this Asana account has several workspaces (Acme, Beta): set the one to use in Settings → Issues → Asana"
)]
#[case::no_match("Gamma", ME_TWO, "no Asana workspace named Gamma; yours are Acme, Beta")]
#[case::none_at_all(
    "",
    r#"{"data":{"gid":"u-me","name":"Ana","workspaces":[]}}"#,
    "this Asana account belongs to no workspace"
)]
#[case::not_asana("", "{}", "{url} does not look like the Asana API")]
fn a_workspace_that_cannot_be_chosen_says_why(#[case] setting: &str, #[case] me: &'static str, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /users/me", 200, me)]);
    let err = api(&server, setting, &[]).whoami().err().map(|e| e.to_string());
    assert_eq!(err, Some(message.replace("{url}", &server.url())));
}

const TWINS: &str = r#"{"data":{"gid":"u-me","name":"Ana Pérez","workspaces":[{"gid":"w-1","name":"Acme"},{"gid":"w-2","name":"Acme"}]}}"#;

#[test]
fn a_workspace_chosen_by_id_stays_that_one_when_another_has_the_same_name() {
    let server = FakeHttp::start(vec![
        ("GET /users/me", 200, TWINS.to_string()),
        ("GET /workspaces/w-2/users", 200, USERS.to_string()),
        ("GET /tasks", 200, tasks(&[], None)),
    ]);

    api(&server, "w-2", &[]).list(&Query::default()).expect("list");

    assert!(decoded(&server.request(2)).contains("workspace=w-2"), "{}", server.request(2));
    let twins = [("w-1".to_string(), "Acme".to_string()), ("w-2".to_string(), "Acme".to_string())];
    assert_eq!(pick_workspace("w-2", &twins), Ok("w-2".to_string()));
}

#[test]
fn a_name_shared_by_two_workspaces_asks_for_an_id() {
    let server = FakeHttp::start(vec![("GET /users/me", 200, TWINS.to_string())]);
    let err = api(&server, "acme", &[]).whoami().err().map(|e| e.to_string());
    let expected = "several of your Asana workspaces are named acme: use the ID of one, w-1 or w-2";
    assert_eq!(err.as_deref(), Some(expected));
}

#[rstest]
#[case::unauthorized(401, "{}", "Asana rejected the access token")]
#[case::forbidden(403, "{}", "Asana rejected the access token")]
#[case::rate_limited(429, "{}", "Asana rate limit reached, try again in a minute")]
#[case::paid_plan(402, r#"{"errors":[{"message":"premium"}]}"#, "this needs a paid Asana plan")]
#[case::no_user(404, "{}", "no Asana user was found for this access token")]
#[case::error_body(
    400,
    r#"{"errors":[{"message":"limit: Must be greater than 0"}]}"#,
    "Asana answered: limit: Must be greater than 0"
)]
#[case::empty_body(502, "", "Asana answered 502")]
fn failures_say_why(#[case] status: u16, #[case] body: &'static str, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /users/me", status, body)]);
    let err = api(&server, "", &[]).whoami().err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(message));
}

#[test]
fn a_redirect_is_not_followed_so_the_token_stays_on_the_server() {
    let (server, elsewhere) = redirected("GET /users/me");
    let err = api(&server, "", &[]).whoami().err().map(|e| e.to_string());
    assert!(err.as_deref().is_some_and(|e| e.starts_with("Asana answered 302 with a redirect")), "{err:?}");
    assert_eq!(elsewhere.requests(), Vec::<String>::new());
}

#[test]
fn people_lists_the_workspace_users_by_name() {
    let server = FakeHttp::start(vec![("GET /users/me", 200, ME), ("GET /workspaces/w-1/users", 200, USERS)]);

    let people = api(&server, "", &[]).people().expect("people");

    let expected = [
        Person { handle: "Ana Pérez".into(), name: "ana@acme.dev".into(), id: Some("u-me".into()) },
        Person { handle: "Luis".into(), name: "luis@acme.dev".into(), id: Some("u-2".into()) },
    ];
    assert_eq!(people, expected);
    let sent = decoded(&server.request(1));
    assert!(sent.contains("limit=100") && sent.contains("opt_fields=name,email"), "{sent}");
}

#[test]
fn people_follow_the_next_page_and_stop_after_twenty() {
    let more = r#"{"data":[{"gid":"u-2","name":"Luis"}],"next_page":{"offset":"o1"}}"#;
    let server = FakeHttp::start(vec![("GET /users/me", 200, ME), ("GET /workspaces/w-1/users", 200, more)]);

    api(&server, "", &[]).people().expect("people");

    assert_eq!(server.requests().len(), 1 + PEOPLE_PAGES);
    assert!(decoded(&server.request(2)).contains("offset=o1"), "{}", server.request(2));
}

mod list {
    use super::*;

    fn tasks_sent(query: &Query, projects: &[&str]) -> String {
        let pages = if projects.is_empty() {
            mine(tasks(&[], None))
        } else {
            vec![("GET /projects/p1/tasks", 200, tasks(&[], None))]
        };
        let (_, server) = listed(query, projects, pages);
        decoded(&server.request(2))
    }

    #[test]
    fn turns_tasks_into_issues() {
        let (listed, _server) = listed(&Query::default(), &[], mine(tasks(&[TASK], None)));

        let expected = Issue {
            source: Source::Asana,
            number: 1_200_000_000_000_042,
            key: "ENG-42".into(),
            title: "Login redirect loops".into(),
            url: "https://app.asana.com/1/1/project/9/task/1200000000000042".into(),
            state: "In Progress".into(),
            closed: false,
            labels: vec!["bug".into(), "p1".into()],
            assignees: vec!["Luis".into()],
            author: "Ana Pérez".into(),
            updated_at: "2026-09-19T14:00:00.000Z".into(),
        };
        assert_eq!(listed.issues, [expected]);
        assert_eq!(listed.account.map(|a| a.describe()).as_deref(), Some("@Ana Pérez in Acme"));
    }

    #[test]
    fn a_task_without_an_id_field_is_keyed_by_its_gid() {
        let (listed, _server) = listed(&Query::default(), &[], mine(tasks(&[&task("1200000000000007", "")], None)));
        let issue = &listed.issues[0];

        assert_eq!((issue.key.as_str(), issue.number), ("1200000000000007", 1_200_000_000_000_007));
        assert_eq!((issue.state.as_str(), issue.author.as_str()), ("open", "Ana Pérez"));
        assert_eq!(branch(issue), "asana-1200000000000007-task-1200000000000007");
    }

    #[test]
    fn a_task_with_an_id_field_branches_by_it() {
        let (listed, _server) = listed(&Query::default(), &[], mine(tasks(&[TASK], None)));
        assert_eq!(branch(&listed.issues[0]), "ENG-42-login-redirect-loops");
    }

    #[test]
    fn a_completed_task_is_closed_whatever_its_section() {
        let done = TASK.replace(r#""completed":false"#, r#""completed":true"#);
        let query = Query { closed: true, ..Query::default() };
        let (listed, _server) = listed(&query, &[], mine(tasks(&[&done], None)));
        assert_eq!((listed.issues[0].state.as_str(), listed.issues[0].closed), ("completed", true));
    }

    #[test]
    fn without_projects_it_asks_for_my_tasks_in_the_workspace() {
        let sent = tasks_sent(&Query::default(), &[]);
        assert!(sent.starts_with("GET /tasks?"), "{sent}");
        assert!(sent.contains("assignee=me") && sent.contains("workspace=w-1") && sent.contains("limit=100"), "{sent}");
        assert!(sent.contains("opt_fields=name,completed,modified_at"), "{sent}");
    }

    #[test]
    fn with_projects_it_asks_each_one_and_merges_them_newest_first_without_repeats() {
        let old = task("1", r#","modified_at":"2026-01-01T00:00:00.000Z""#);
        let new = task("2", r#","modified_at":"2026-05-01T00:00:00.000Z""#);
        let pages = vec![
            ("GET /projects/p1/tasks", 200, tasks(&[&old, &new], None)),
            ("GET /projects/p2/tasks", 200, tasks(&[&new], None)),
        ];

        let (listed, server) = listed(&Query::default(), &["p1", "p2"], pages);

        assert_eq!(titles(&listed), ["Task 2", "Task 1"]);
        assert!(decoded(&server.request(2)).starts_with("GET /projects/p1/tasks?"));
        assert!(decoded(&server.request(3)).starts_with("GET /projects/p2/tasks?"));
        assert!(!decoded(&server.request(2)).contains("assignee="));
    }

    #[test]
    fn at_most_ten_projects_are_asked() {
        let gids: Vec<String> = (1..=12).map(|n| format!("p{n}")).collect();
        let gids: Vec<&str> = gids.iter().map(String::as_str).collect();
        let pages = gids
            .iter()
            .map(|gid| (&*Box::leak(format!("GET /projects/{gid}/tasks").into_boxed_str()), 200, tasks(&[], None)))
            .collect();
        let (_, server) = listed(&Query::default(), &gids, pages);
        assert_eq!(server.requests().len(), 2 + PROJECTS);
    }

    #[test]
    fn open_tasks_ask_for_the_incomplete_ones_since_now() {
        for (projects, route) in [(vec![], "GET /tasks?"), (vec!["p1"], "GET /projects/p1/tasks?")] {
            let sent = tasks_sent(&Query::default(), &projects);
            assert!(sent.starts_with(route), "{sent}");
            let since = sent.split("completed_since=").nth(1).and_then(|rest| rest.split(['&', ' ']).next());
            let since = since.expect("completed_since");
            assert!(since.len() == 20 && since.ends_with('Z') && since.contains('T'), "{since}");
            assert!(since.starts_with("20"), "{since}");
        }
    }

    #[test]
    fn closed_tasks_send_no_completed_since() {
        let query = Query { closed: true, ..Query::default() };
        for projects in [vec![], vec!["p1"]] {
            assert!(!tasks_sent(&query, &projects).contains("completed_since"));
        }
    }

    #[test]
    fn it_follows_the_next_page() {
        let first = tasks(&[&task("1", "")], Some("o-2"));
        let second = tasks(&[&task("2", "")], None);
        let pages = vec![("GET /tasks", 200, first), ("GET /tasks", 200, second)];

        let (listed, server) = listed(&Query::default(), &[], pages);

        assert_eq!(listed.issues.len(), 2);
        assert!(!decoded(&server.request(2)).contains("offset="));
        assert!(decoded(&server.request(3)).contains("offset=o-2"), "{}", server.request(3));
    }

    #[test]
    fn it_stops_after_ten_pages_per_source() {
        let endless = tasks(&[&task("1", "")], Some("again"));
        let (listed, server) = listed(&Query::default(), &[], mine(endless));
        assert_eq!(server.requests().len(), 2 + LIST_PAGES);
        assert_eq!(listed.issues.len(), 1);
    }

    #[test]
    fn it_stops_asking_once_a_hundred_tasks_are_kept() {
        let rows: Vec<String> = (0..LIMIT).map(|n| task(&format!("{n}"), "")).collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let (listed, server) = listed(&Query::default(), &[], mine(tasks(&rows, Some("more"))));
        assert_eq!((listed.issues.len(), server.requests().len()), (LIMIT, 3));
    }

    #[test]
    fn milestones_approvals_and_separators_are_left_out() {
        let milestone = task("2", "").replace("default_task", "milestone");
        let approval = task("3", "").replace("default_task", "approval");
        let separator = task("4", r#","is_rendered_as_separator":true"#);
        let rows = [task("1", ""), milestone, approval, separator];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();

        let (listed, _server) = listed(&Query::default(), &[], mine(tasks(&rows, None)));

        assert_eq!(titles(&listed), ["Task 1"]);
    }

    #[test]
    fn an_ordinary_subtask_is_left_out_in_both_modes() {
        let subtask = task("2", r#","parent":{"gid":"1","name":"Task 1"}"#);
        let rows = [task("1", ""), subtask];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();

        let (yours, _server) = listed(&Query::default(), &[], mine(tasks(&rows, None)));
        let pages = vec![("GET /projects/p1/tasks", 200, tasks(&rows, None))];
        let (projects, server) = listed(&Query::default(), &["p1"], pages);

        assert_eq!((titles(&yours), titles(&projects)), (vec!["Task 1"], vec!["Task 1"]));
        assert!(decoded(&server.request(2)).contains("parent"), "{}", server.request(2));
    }

    fn assigned_to_luis() -> String {
        task("1", r#","assignee":{"gid":"u-2","name":"Luis"}"#)
    }

    fn page_of_two() -> Vec<Route> {
        let by_luis = task("2", r#","created_by":{"gid":"u-2"}"#);
        let rows = [assigned_to_luis(), by_luis];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        vec![("GET /projects/p1/tasks", 200, tasks(&rows, None))]
    }

    #[rstest]
    #[case::anyone([Who::Anyone, Who::Anyone], &["Task 1", "Task 2"])]
    #[case::assignee_a_user([Who::User { id: "u-2".into(), name: "Luis".into() }, Who::Anyone], &["Task 1"])]
    #[case::assignee_me([Who::Me, Who::Anyone], &[])]
    #[case::creator_me([Who::Anyone, Who::Me], &["Task 1"])]
    #[case::creator_a_user([Who::Anyone, Who::User { id: "u-2".into(), name: "Luis".into() }], &["Task 2"])]
    fn people_filter_the_tasks_of_projects_locally(#[case] people: [Who; 2], #[case] expected: &[&str]) {
        let (listed, server) = listed(&Query { closed: false, people }, &["p1"], page_of_two());
        assert_eq!(titles(&listed), expected);
        assert!(!server.request(2).contains("assignee="), "{}", server.request(2));
    }

    #[test]
    fn my_tasks_filter_by_creator_too() {
        let query = Query { closed: false, people: [Who::Me, Who::User { id: "u-2".into(), name: "Luis".into() }] };
        let (listed, _server) = listed(&query, &[], mine(page_of_two().remove(0).2));
        assert_eq!(titles(&listed), ["Task 2"]);
    }

    #[rstest]
    #[case::a_user(Who::User { id: "u-2".into(), name: "Luis".into() })]
    #[case::a_handle(Who::Person("luis".into()))]
    fn my_tasks_cannot_be_filtered_by_someone_else(#[case] who: Who) {
        let server = FakeHttp::start(vec![
            ("GET /users/me", 200, ME.to_string()),
            ("GET /workspaces/w-1/users", 200, USERS.to_string()),
        ]);

        let err = api(&server, "", &[]).list(&Query { closed: false, people: [who, Who::Anyone] }).err();

        let message = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("this list is your tasks") && message.contains("set Asana projects"), "{message}");
        assert!(server.requests().iter().all(|r| !r.starts_with("GET /tasks")));
    }
}

mod read {
    use super::*;

    const STORIES: &str = r#"{"data":[
        {"resource_subtype":"assigned","text":"Ana assigned to Luis","created_at":"2026-09-02T10:00:00.000Z","created_by":{"name":"Ana Pérez"}},
        {"resource_subtype":"comment_added","html_text":"<body>Later <strong>one</strong></body>","text":"Later one","created_at":"2026-09-21T10:00:00.000Z","created_by":{"name":"Luis"}},
        {"resource_subtype":"comment_added","html_text":"","text":"earlier plain","created_at":"2026-09-20T10:00:00.000Z","created_by":{"name":"Ana Pérez"}},
        {"resource_subtype":"comment_added","html_text":"<body></body>","text":"","created_at":"2026-09-22T10:00:00.000Z"}],
        "next_page":null}"#;

    fn detail_node(notes_html: &str) -> String {
        let mut node: Value = serde_json::from_str(TASK).expect("task");
        node["html_notes"] = Value::String(notes_html.into());
        node["notes"] = Value::String("plain notes".into());
        node["due_on"] = Value::String("2026-10-01".into());
        serde_json::json!({ "data": node }).to_string()
    }

    fn served(key: &str, node: String, extra: Vec<Route>) -> (Detail, FakeHttp) {
        let mut routes = vec![
            ("GET /users/me", 200, ME.to_string()),
            ("GET /workspaces/w-1/users", 200, USERS.to_string()),
            ("GET /tasks/1200000000000042", 200, node),
            ("GET /tasks/1200000000000042/stories", 200, STORIES.to_string()),
        ];
        routes.extend(extra);
        let server = FakeHttp::start(routes);
        let detail = api(&server, "", &[]).view(key).expect("view");
        (detail, server)
    }

    #[test]
    fn by_gid_reads_the_task_and_keeps_only_comments() {
        let (detail, server) = served("1200000000000042", detail_node("<body><p>Fix it</p></body>"), vec![]);

        assert_eq!(detail.body, "Fix it");
        let comments: Vec<(&str, &str, &str)> =
            detail.comments.iter().map(|c| (c.author.as_str(), c.created_at.as_str(), c.body.as_str())).collect();
        assert_eq!(
            comments,
            [
                ("Ana Pérez", "2026-09-20T10:00:00.000Z", "earlier plain"),
                ("Luis", "2026-09-21T10:00:00.000Z", "Later **one**"),
            ]
        );
        let task = server
            .requests()
            .iter()
            .map(|r| decoded(r))
            .find(|r| r.starts_with("GET /tasks/1200000000000042?"))
            .unwrap_or_default();
        assert!(task.contains("html_notes") && task.contains("memberships.project.name"), "{task}");
        let stories = server.requests().iter().map(|r| decoded(r)).find(|r| r.contains("/stories")).unwrap_or_default();
        assert!(stories.contains("resource_subtype=comment_added") && stories.contains("limit=100"), "{stories}");
    }

    #[test]
    fn the_info_line_gathers_section_due_author_assignee_and_projects() {
        let (detail, _server) = served("1200000000000042", detail_node("<body>x</body>"), vec![]);
        assert_eq!(detail.info, ["In Progress", "due 2026-10-01", "created by @Ana Pérez", "assigned to @Luis", "Web"]);
    }

    #[test]
    fn by_custom_id_asks_the_workspace_without_query_parameters_then_the_task() {
        let found = r#"{"data":{"gid":"1200000000000042","name":"x"}}"#.to_string();
        let route = vec![("GET /workspaces/w-1/tasks/custom_id/ENG-42", 200, found)];
        let (detail, server) = served("ENG-42", detail_node("<body>x</body>"), route);

        assert_eq!(detail.body, "x");
        let lookup = server.requests().into_iter().find(|r| r.contains("custom_id")).unwrap_or_default();
        assert!(lookup.starts_with("GET /workspaces/w-1/tasks/custom_id/ENG-42 "), "{lookup}");
    }

    #[test]
    fn a_missing_custom_id_says_so() {
        let server = FakeHttp::start(vec![
            ("GET /users/me", 200, ME.to_string()),
            ("GET /workspaces/w-1/users", 200, USERS.to_string()),
            ("GET /workspaces/w-1/tasks/custom_id/ENG-9", 404, "{}".to_string()),
        ]);
        let err = api(&server, "", &[]).view("ENG-9").err().map(|e| e.to_string());
        assert_eq!(err.as_deref(), Some("task ENG-9 not found"));
    }

    #[test]
    fn mentions_inaccessible_links_and_images_become_text() {
        let html = r#"<body>Hi <a href="https://app.asana.com/0/1/2" data-asana-gid="2" data-asana-type="user">@Tim</a>,
            see <a href="https://app.asana.com/0/9/9" data-asana-accessible="false">Private task</a>
            and <a href="https://example.com/a">the docs</a><img data-asana-gid="8" src="https://x.dev/e.png?exp=1"></body>"#;

        let (detail, _server) = served("1200000000000042", detail_node(html), vec![]);

        assert_eq!(detail.body, "Hi @Tim, see Private task and [the docs](https://example.com/a)[image]");
    }

    #[test]
    fn empty_rich_text_falls_back_to_the_plain_notes() {
        let (detail, _server) = served("1200000000000042", detail_node("<body></body>"), vec![]);
        assert_eq!(detail.body, "plain notes");
    }

    #[test]
    fn find_returns_the_issue_by_key() {
        let found = r#"{"data":{"gid":"1200000000000042"}}"#.to_string();
        let route = vec![("GET /workspaces/w-1/tasks/custom_id/ENG-42", 200, found)];
        let server = FakeHttp::start(
            [
                vec![
                    ("GET /users/me", 200, ME.to_string()),
                    ("GET /workspaces/w-1/users", 200, USERS.to_string()),
                    ("GET /tasks/1200000000000042", 200, detail_node("<body>x</body>")),
                ],
                route,
            ]
            .concat(),
        );

        let issue = api(&server, "", &[]).find("ENG-42").expect("find");

        assert_eq!((issue.key.as_str(), issue.title.as_str()), ("ENG-42", "Login redirect loops"));
        assert_eq!(issue.author, "Ana Pérez");
    }
}

#[rstest]
#[case::bare("1200000000000042", &["1200000000000042"])]
#[case::several("1 2,3\n4", &["1", "2", "3", "4"])]
#[case::new_url("https://app.asana.com/1/12345/project/678/list/910", &["678"])]
#[case::new_url_with_query("https://app.asana.com/1/12345/project/678?focus=true", &["678"])]
#[case::old_url("https://app.asana.com/0/678/910", &["678"])]
#[case::old_url_without_a_task("https://app.asana.com/0/678/list", &["678"])]
#[case::old_url_with_a_trailing_f("https://app.asana.com/0/678/910/f", &["678"])]
#[case::mixed("https://app.asana.com/0/678/list, 99", &["678", "99"])]
#[case::a_task_link_is_no_project("https://app.asana.com/1/12345/task/910", &[])]
#[case::words_are_no_project("my project", &[])]
#[case::nothing("  ", &[])]
fn project_gids_reads_ids_and_links(#[case] input: &str, #[case] expected: &[&str]) {
    assert_eq!(project_gids(input), expected);
}
