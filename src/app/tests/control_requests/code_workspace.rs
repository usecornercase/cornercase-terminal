use super::*;

const SHELL: &str = if cfg!(unix) { "/bin/sh" } else { "cmd.exe" };

const TEAM: &str = r#"{
    // the team's repositories
    "folders": [
        { "path": "open", }, /* already a project */
        { "path": "api", "name": "Backend", },
        { "path": "gone" },
    ],
    "settings": { "url": "http://example.com/x", },
}"#;

struct Setup {
    app: App,
    _rx: Receiver<AppEvent>,
    root: PathBuf,
    _tmp: TempDir,
}

impl Setup {
    fn new(folders: &[&str]) -> Self {
        let tmp = TempDir::new();
        let root = tmp.path().to_path_buf();
        for folder in folders {
            std::fs::create_dir_all(root.join(folder)).expect("create folder");
        }
        let (app, rx) = new_app_with_shell(SHELL, no_config());
        Self { app, _rx: rx, root, _tmp: tmp }
    }

    fn opened(&mut self, folder: &str) -> u64 {
        self.app.open_project(self.root.join(folder), AREA).expect("open project");
        self.shown()
    }

    fn shown(&self) -> u64 {
        self.app.project().expect("a project").id
    }

    fn file(&self, name: &str, text: &str) -> Value {
        let file = self.root.join(name);
        std::fs::write(&file, text).expect("write workspace");
        json!(file)
    }

    fn import(&mut self, text: &str, args: &Value) -> wire::ImportedWorkspace {
        let mut args = args.clone();
        args["path"] = self.file("team.code-workspace", text);
        serde_json::from_value(ok(&mut self.app, "open", &args)).expect("an answer")
    }

    fn group_named(&self, name: &str) -> Option<u64> {
        self.app.groups.iter().find(|g| g.entry.name == name).map(|g| g.id)
    }

    fn in_group(&self, group: Option<u64>) -> Vec<PathBuf> {
        self.app.projects.iter().filter(|p| group.is_some() && p.group == group).map(|p| p.path.clone()).collect()
    }

    fn named(&self, id: u64) -> Option<&str> {
        self.app.projects.iter().find(|p| p.id == id).and_then(|p| p.name.as_deref())
    }
}

mod a_workspace_file {
    use super::*;

    #[test]
    fn becomes_a_group_of_its_folders_and_the_window_stays_where_it_was() {
        let mut s = Setup::new(&["here", "open", "api"]);
        let open = s.opened("open");
        let here = s.opened("here");

        let imported = s.import(TEAM, &json!({}));

        let group = s.group_named("team");
        let api = imported.projects[1];
        assert_eq!((imported.group, imported.name.as_str()), (group, "team"));
        assert_eq!(imported.projects[0], open);
        assert_eq!((imported.missing, imported.failed, imported.projection), (1, 0, None));
        assert_eq!(s.in_group(group), [s.root.join("open"), s.root.join("api")]);
        assert_eq!((s.named(open), s.named(api)), (None, Some("Backend")));
        assert_eq!(s.shown(), here);
        assert!(toast(&s.app).is_none() && s.app.overlay.is_none());
    }

    #[test]
    fn with_focus_shows_its_first_project() {
        let mut s = Setup::new(&["here", "open", "api"]);
        s.opened("here");

        let imported = s.import(TEAM, &json!({"focus": true}));

        assert_eq!(s.shown(), imported.projects[0]);
        assert_eq!(s.app.project().map(|p| p.path.clone()), Some(s.root.join("open")));
    }

    #[test]
    fn with_focus_in_a_view_moves_that_view_and_not_the_window() {
        let mut s = Setup::new(&["here", "open"]);
        let here = s.opened("here");
        let text = json!({"contract": 1, "command": "view", "args": {}}).to_string();
        s.app.request(CLIENT + 1, &text, Some(AREA), Instant::now());
        let answers = s.app.take_answers();
        let view = reply_of(&answers.first().expect("an answer").1).expect("a view")["view"]["id"].clone();
        let file = s.file("team.code-workspace", r#"{"folders": [{"path": "open"}]}"#);

        let args = json!({"path": file, "focus": true});
        let text = json!({"contract": 1, "view": view, "command": "open", "args": args}).to_string();
        s.app.request(CLIENT, &text, Some(AREA), Instant::now());

        let imported = reply_of(&s.app.take_answers().remove(0).1).expect("an answer");
        assert!(imported["projection"].is_u64(), "{imported}");
        assert_eq!(s.shown(), here);
    }

    #[test]
    fn reuses_a_group_with_its_name_and_brings_in_folders_added_since() {
        let mut s = Setup::new(&["api", "web"]);
        let first = s.import(r#"{"folders": [{"path": "api"}]}"#, &json!({}));

        let second = s.import(r#"{"folders": [{"path": "api"}, {"path": "web"}]}"#, &json!({}));

        assert_eq!((first.group, second.group), (s.group_named("team"), s.group_named("team")));
        assert_eq!(second.projects.first(), first.projects.first());
        assert_eq!((s.app.groups.len(), s.app.projects.len(), second.projects.len()), (1, 2, 2));
    }

    #[test]
    fn counts_a_folder_listed_twice_once() {
        let mut s = Setup::new(&["api"]);

        let imported = s.import(r#"{"folders": [{"path": "api"}, {"path": "./api"}]}"#, &json!({}));

        assert_eq!((imported.projects.len(), s.app.projects.len()), (1, 1));
    }

    #[test]
    fn keeps_a_name_given_in_cornercase() {
        let mut s = Setup::new(&["api"]);
        let api = s.opened("api");
        s.app.projects[0].name = Some("mine".into());

        s.import(r#"{"folders": [{"path": "api", "name": "Backend"}]}"#, &json!({}));

        assert_eq!(s.named(api), Some("mine"));
    }

    #[test]
    fn opens_a_project_being_closed_again_as_a_folder_does() {
        let mut s = Setup::new(&["api"]);
        let old = s.opened("api");
        s.app.close_project(old);

        let imported = s.import(r#"{"folders": [{"path": "api"}]}"#, &json!({}));

        assert!(imported.projects.len() == 1 && imported.projects[0] != old, "{imported:?}");
    }

    #[test]
    fn with_no_folder_found_leaves_no_group_and_says_how_many_were_gone() {
        let mut s = Setup::new(&[]);
        let gone = crate::test_util::os_path("/nonexistent/cornercase-gone");
        let text = format!(r#"{{"folders": [{{"path": "gone"}}, {{"path": "{gone}"}}]}}"#);

        let imported = s.import(&text, &json!({}));

        assert_eq!((imported.group, imported.projects.len(), imported.missing), (None, 0, 2));
        assert_eq!((imported.name.as_str(), s.app.groups.len()), ("team", 0));
    }

    #[test]
    fn is_read_from_the_home_folder_of_the_server() {
        let mut s = Setup::new(&["api"]);
        s.app.home = Some(s.root.clone());
        s.file("team.code-workspace", r#"{"folders": [{"path": "api"}]}"#);

        let imported = ok(&mut s.app, "open", &json!({"path": "~/team.code-workspace"}));

        assert_eq!(imported["name"], "team");
        assert_eq!(s.in_group(s.group_named("team")), [s.root.join("api")]);
    }

    #[test]
    fn that_cannot_be_read_is_refused_and_changes_nothing() {
        let mut s = Setup::new(&["api"]);
        let file = s.file("team.code-workspace", "{ nope");

        let refused = refused(&mut s.app, "open", &json!({"path": file}));

        assert_eq!(refused.code, "failed");
        assert!(refused.message.starts_with("cannot read"), "{}", refused.message);
        assert!(s.app.groups.is_empty() && s.app.projects.is_empty());
    }

    #[test]
    fn needs_a_window_to_size_its_shells() {
        let mut s = Setup::new(&["api"]);
        let file = s.file("team.code-workspace", r#"{"folders": [{"path": "api"}]}"#);

        let text = json!({"contract": 1, "command": "open", "args": {"path": file}}).to_string();
        s.app.request(CLIENT, &text, None, Instant::now());

        let refused = reply_of(&s.app.take_answers().remove(0).1).expect_err("no size");
        assert!(refused.message.contains("start-server"), "{}", refused.message);
        assert!(s.app.groups.is_empty() && s.app.projects.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn counts_a_folder_whose_shell_cannot_start_and_opens_the_rest() {
        let mut s = Setup::new(&["api"]);
        let locked = Locked::new();
        let text = format!(r#"{{"folders": [{{"path": "{}"}}, {{"path": "api"}}]}}"#, locked.path().display());

        let imported = s.import(&text, &json!({}));

        assert_eq!((imported.projects.len(), imported.missing, imported.failed), (1, 0, 1));
        assert_eq!(s.in_group(imported.group), [s.root.join("api")]);
    }

    #[cfg(unix)]
    #[test]
    fn by_a_link_takes_the_links_name_and_folder_as_the_picker_does() {
        let mut s = Setup::new(&["links/api", "elsewhere"]);
        s.file("elsewhere/real.code-workspace", r#"{"folders": [{"path": "api"}]}"#);
        let link = s.root.join("links/team.code-workspace");
        std::os::unix::fs::symlink(s.root.join("elsewhere/real.code-workspace"), &link).expect("link");

        let imported = ok(&mut s.app, "open", &json!({"path": link}));

        assert_eq!((&imported["name"], &imported["missing"]), (&json!("team"), &json!(0)));
        assert_eq!(s.in_group(s.group_named("team")), [s.root.join("links/api")]);
    }

    #[cfg(not(unix))]
    #[test]
    fn by_its_verbatim_path_reads_its_folders_as_plain_ones() {
        let mut s = Setup::new(&["api"]);
        s.file("team.code-workspace", r#"{"folders": [{"path": "api"}]}"#);
        let file = crate::test_util::verbatim(&s.root.join("team.code-workspace"));

        let imported = ok(&mut s.app, "open", &json!({"path": file}));

        assert_eq!(imported["missing"], 0);
        assert_eq!(s.in_group(s.group_named("team")), [s.root.join("api")]);
    }
}

mod into_a_group {
    use super::*;

    fn group(s: &mut Setup, name: &str) -> u64 {
        ok(&mut s.app, "group-new", &json!({"name": name}))["group"].as_u64().expect("the group's id")
    }

    fn folder_into(s: &mut Setup, folder: &str, group: u64) -> Value {
        let path = s.root.join(folder);
        ok(&mut s.app, "open", &json!({"path": path, "into": group}))
    }

    #[test]
    fn a_workspace_file_goes_there_and_no_group_takes_its_name() {
        let mut s = Setup::new(&["api"]);
        let other = group(&mut s, "other");

        let imported = s.import(r#"{"folders": [{"path": "api"}]}"#, &json!({"into": other}));

        assert_eq!((imported.group, imported.name.as_str()), (Some(other), "other"));
        assert_eq!(s.in_group(Some(other)), [s.root.join("api")]);
        assert_eq!(s.group_named("team"), None);
    }

    #[test]
    fn a_group_that_gets_nothing_stays() {
        let mut s = Setup::new(&[]);
        let other = group(&mut s, "other");

        let imported = s.import(r#"{"folders": [{"path": "gone"}]}"#, &json!({"into": other}));

        assert_eq!((imported.group, imported.projects.len(), imported.missing), (Some(other), 0, 1));
        assert_eq!(s.app.groups.len(), 1);
    }

    #[test]
    fn a_new_folder_opens_there_and_answers_as_any_folder() {
        let mut s = Setup::new(&["api"]);
        let other = group(&mut s, "other");

        let answer = folder_into(&mut s, "api", other);

        let id = s.app.projects[0].id;
        assert_eq!(answer["project"], id);
        assert_eq!((answer.get("projects"), answer.get("group")), (None, None));
        assert_eq!(s.in_group(Some(other)), [s.root.join("api")]);
        assert_eq!(s.shown(), id);
    }

    #[test]
    fn an_open_folder_moves_there_and_the_window_stays_where_it_was() {
        let mut s = Setup::new(&["api", "here"]);
        let api = s.opened("api");
        let here = s.opened("here");
        let other = group(&mut s, "other");

        let answer = folder_into(&mut s, "api", other);

        assert_eq!(answer["project"], api);
        assert_eq!(s.in_group(Some(other)), [s.root.join("api")]);
        assert_eq!(s.shown(), here);
    }

    #[test]
    fn a_folded_group_stays_folded_without_focus() {
        let mut s = Setup::new(&["api"]);
        let other = group(&mut s, "other");
        ok(&mut s.app, "collapse", &json!({"item": {"group": other}}));

        folder_into(&mut s, "api", other);
        s.import(r#"{"folders": [{"path": "api"}]}"#, &json!({"into": other}));

        assert!(s.app.group(other).is_some_and(|g| g.collapsed));
    }

    #[test]
    fn a_group_being_deleted_is_kept_as_adding_a_project_keeps_it() {
        let mut s = Setup::new(&["api"]);
        let other = group(&mut s, "other");
        s.app.deleting_groups.push(other);

        folder_into(&mut s, "api", other);

        assert_eq!(s.app.deleting_groups, [] as [u64; 0]);
    }

    #[rstest::rstest]
    #[case::a_folder("api")]
    #[case::a_workspace_file("team.code-workspace")]
    fn a_group_that_is_not_there_is_refused_before_anything_opens(#[case] name: &str) {
        let mut s = Setup::new(&["api"]);
        s.file("team.code-workspace", r#"{"folders": [{"path": "api"}]}"#);
        let path = s.root.join(name);

        let refused = refused(&mut s.app, "open", &json!({"path": path, "into": 999}));

        assert_eq!(refused.code, "not-found");
        assert!(s.app.projects.is_empty() && s.app.groups.is_empty());
    }
}

mod what_stays_a_folder {
    use super::*;

    #[test]
    fn a_folder_answers_byte_for_byte_as_before() {
        let mut s = Setup::new(&["api"]);
        let path = s.root.join("api");
        let open = Command::Open(wire::Open { path, ..wire::Open::default() });

        ask(&mut s.app, None, open);

        let at = s.app.shown_ids(0);
        let [project, workspace, tab, pane] = [at.project, at.workspace, at.tab, at.pane].map(|id| id.expect("an id"));
        let answer = s.app.take_answers().remove(0).1;
        assert_eq!(
            answer,
            format!(r#"{{"ok":{{"pane":{pane},"project":{project},"tab":{tab},"workspace":{workspace}}}}}"#)
        );
    }

    #[test]
    fn a_folder_named_like_a_workspace_file_opens_as_a_folder() {
        let mut s = Setup::new(&["team.code-workspace"]);
        let path = s.root.join("team.code-workspace");

        let answer = ok(&mut s.app, "open", &json!({"path": path}));

        assert!(answer["project"].is_u64(), "{answer}");
        assert!(s.app.groups.is_empty());
    }

    #[test]
    fn another_file_is_refused_as_before() {
        let mut s = Setup::new(&[]);
        let file = s.file("team.json", r#"{"folders": []}"#);

        let refused = refused(&mut s.app, "open", &json!({"path": file}));

        let shown = s.root.join("team.json");
        assert_eq!(
            (refused.code.as_str(), refused.message),
            ("failed", format!("`{}` is not a folder", shown.display()))
        );
    }

    #[test]
    fn a_workspace_file_that_is_not_there_is_refused_as_any_path() {
        let mut s = Setup::new(&[]);
        let path = s.root.join("team.code-workspace");

        let refused = refused(&mut s.app, "open", &json!({"path": path}));

        assert!(refused.message.starts_with("cannot open"), "{}", refused.message);
    }
}
