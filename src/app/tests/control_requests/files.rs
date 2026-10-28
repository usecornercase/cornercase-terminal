use std::os::unix::fs::symlink;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rstest::rstest;

use super::*;
use crate::files::disk::MAX_BYTES;
use crate::project::Hierarchy;

fn project() -> TempDir {
    let repo =
        git_repo(&[(".gitignore", "*.log\n"), ("src/a.rs", "fn a() {}\n"), ("notes.txt", "one\n"), ("bin.dat", "x")]);
    let dir = repo.path();
    std::fs::write(dir.join("notes.txt"), "one\nTWO\n").expect("edit");
    std::fs::write(dir.join("new.txt"), "n\n").expect("write");
    std::fs::write(dir.join(".env"), "A=1\n").expect("write");
    std::fs::write(dir.join("run.log"), "log\n").expect("write");
    std::fs::write(dir.join("bin.dat"), [b'a', 0, b'b']).expect("write");
    std::fs::create_dir_all(dir.join("fresh/deep")).expect("mkdir");
    std::fs::write(dir.join("fresh/deep/x.txt"), "x\n").expect("write");
    repo
}

fn started(repo: &TempDir) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.clients.file_streams.set_every(Duration::ZERO);
    (app, rx)
}

fn workspace_id(app: &App) -> u64 {
    app.projects.workspaces().next().expect("a workspace").id
}

fn entries(folder: &Value) -> Vec<(String, String, String)> {
    let text = |value: &Value| value.as_str().unwrap_or("-").to_string();
    let rows = folder["entries"].as_array().expect("entries");
    rows.iter().map(|e| (text(&e["name"]), text(&e["kind"]), text(&e["status"]))).collect()
}

fn row(folder: &Value, name: &str) -> (String, String, String) {
    entries(folder).into_iter().find(|(entry, ..)| entry == name).unwrap_or_else(|| panic!("{name} in {folder}"))
}

fn names(folder: &Value) -> Vec<String> {
    entries(folder).into_iter().map(|(name, ..)| name).collect()
}

fn read(app: &mut App, rx: &Receiver<AppEvent>, path: &str) -> Value {
    call_later(app, rx, "files-read", &json!({"path": path})).unwrap_or_else(|e| panic!("{path}: {}", e.message))
}

mod listing {
    use super::*;

    #[test]
    fn the_root_has_its_entries_with_git_marks_and_without_what_git_ignores() {
        let repo = project();
        let (mut app, rx) = started(&repo);

        let list = call_later(&mut app, &rx, "files-list", &json!({})).expect("a listing");

        let root = &list["folders"][0];
        assert_eq!(list["workspace"], workspace_id(&app));
        assert_eq!(
            (root["path"].clone(), root["more"].clone(), root["error"].clone()),
            (json!(""), json!(false), Value::Null)
        );
        assert_eq!(names(root), ["fresh", "src", ".env", ".gitignore", "bin.dat", "new.txt", "notes.txt"]);
        let marks: Vec<String> = entries(root).into_iter().map(|(_, _, status)| status).collect();
        assert_eq!(marks, ["added", "-", "untracked", "-", "modified", "untracked", "modified"]);
    }

    #[test]
    fn a_hidden_file_says_so_and_a_file_says_its_size() {
        let repo = project();
        let (mut app, rx) = started(&repo);

        let list = call_later(&mut app, &rx, "files-list", &json!({})).expect("a listing");

        let env = list["folders"][0]["entries"]
            .as_array()
            .and_then(|e| e.iter().find(|e| e["name"] == ".env"))
            .expect(".env");
        assert_eq!(
            (env["hidden"].clone(), env["bytes"].clone(), env["kind"].clone()),
            (json!(true), json!(4), json!("file"))
        );
        assert!(env["modified"].as_str().is_some_and(|at| at.ends_with('Z')), "{env}");
    }

    #[test]
    fn a_folder_inside_the_workspace_has_marks_relative_to_the_workspace() {
        let repo = project();
        std::fs::write(repo.path().join("src/a.rs"), "fn b() {}\n").expect("edit");
        let (mut app, rx) = started(&repo);

        let list =
            call_later(&mut app, &rx, "files-list", &json!({"folders": ["src", "fresh/./deep"]})).expect("a listing");

        assert_eq!(row(&list["folders"][0], "a.rs").2, "modified");
        assert_eq!(list["folders"][0]["path"], "src");
        assert_eq!(row(&list["folders"][1], "x.txt").2, "untracked");
    }

    #[test]
    fn several_folders_come_in_one_answer_and_a_failed_one_says_why() {
        let repo = project();
        let (mut app, rx) = started(&repo);

        let list = call_later(&mut app, &rx, "files-list", &json!({"folders": ["src", "gone", "notes.txt"]}))
            .expect("a listing");

        let folders = list["folders"].as_array().expect("folders");
        assert_eq!(folders.len(), 3);
        assert_eq!(names(&folders[0]), ["a.rs"]);
        assert_eq!((folders[1]["error"].clone(), folders[1]["entries"].clone()), (json!("no such folder"), json!([])));
        assert_eq!(folders[2]["error"], "not a folder");
    }

    #[test]
    fn an_absolute_path_and_one_in_the_home_folder_need_no_project_and_have_no_marks() {
        let home = TempDir::new();
        std::fs::create_dir(home.path().join("sub")).expect("mkdir");
        std::fs::write(home.path().join("sub/x.txt"), "x").expect("write");
        let (mut app, rx) = new_app(no_config());
        app.home = Some(home.path().to_path_buf());
        let absolute = home.path().join("sub").display().to_string();

        let list =
            call_later(&mut app, &rx, "files-list", &json!({"folders": [absolute, "~/sub"]})).expect("a listing");

        assert_eq!(list["workspace"], Value::Null);
        assert_eq!(entries(&list["folders"][0]), [("x.txt".to_string(), "file".to_string(), "-".to_string())]);
        assert_eq!(list["folders"][1]["path"], "~/sub");
        assert_eq!(names(&list["folders"][1]), ["x.txt"]);
        assert_eq!(list["folders"][1]["dir"], home.path().join("sub").display().to_string());
    }

    #[test]
    fn a_folder_outside_the_workspace_has_no_marks() {
        let repo = project();
        let outside = git_repo(&[("a.txt", "one\n")]);
        std::fs::write(outside.path().join("a.txt"), "two\n").expect("edit");
        let (mut app, rx) = started(&repo);
        let asked = outside.path().display().to_string();

        let list = call_later(&mut app, &rx, "files-list", &json!({"folders": [asked]})).expect("a listing");

        assert_eq!(row(&list["folders"][0], "a.txt").2, "-");
    }

    #[rstest]
    #[case::the_root(json!({}))]
    #[case::a_relative_folder(json!({"folders": ["src"]}))]
    fn without_a_workspace_a_relative_path_is_refused(#[case] args: Value) {
        let (mut app, _rx) = new_app(no_config());

        let refused = refused(&mut app, "files-list", &args);

        assert_eq!(refused.code, "failed");
        assert!(refused.message.contains("no workspace is open here; pass --workspace"), "{}", refused.message);
    }

    #[test]
    fn a_workspace_that_does_not_exist_is_not_found() {
        let repo = project();
        let (mut app, _rx) = started(&repo);

        assert_eq!(refused(&mut app, "files-list", &json!({"workspace": 99})).code, "not-found");
    }
}

mod reading {
    use super::*;

    fn png() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(5, 2).write_to(&mut out, image::ImageFormat::Png).expect("a png");
        out.into_inner()
    }

    fn with_files(repo: &TempDir) {
        std::fs::write(repo.path().join("img.png"), png()).expect("write");
        let big = std::fs::File::create(repo.path().join("big.txt")).expect("create");
        big.set_len(MAX_BYTES + 1).expect("sparse");
    }

    #[rstest]
    #[case::text("src/a.rs", "text")]
    #[case::binary("bin.dat", "binary")]
    #[case::image("img.png", "image")]
    #[case::too_large("big.txt", "too-large")]
    #[case::folder("src", "folder")]
    #[case::missing("gone.txt", "missing")]
    fn tells_what_a_path_is(#[case] path: &str, #[case] kind: &str) {
        let repo = project();
        with_files(&repo);
        let (mut app, rx) = started(&repo);

        let read = read(&mut app, &rx, path);

        assert_eq!((read["kind"].clone(), read["path"].clone()), (json!(kind), json!(path)));
        assert_eq!(read["workspace"], workspace_id(&app));
    }

    #[test]
    fn a_text_has_its_content_language_and_size() {
        let repo = project();
        let (mut app, rx) = started(&repo);

        let read = read(&mut app, &rx, "src/a.rs");

        assert_eq!(
            (read["text"].clone(), read["language"].clone(), read["bytes"].clone()),
            (json!("fn a() {}\n"), json!("rust"), json!(10))
        );
        assert_eq!(read["file"], repo.path().join("src/a.rs").display().to_string());
        assert_eq!(read["image"], Value::Null);
    }

    #[test]
    fn an_image_has_its_format_size_and_the_file_in_base64() {
        let repo = project();
        with_files(&repo);
        let (mut app, rx) = started(&repo);

        let read = read(&mut app, &rx, "img.png");

        let image = &read["image"];
        assert_eq!(
            (image["format"].clone(), image["width"].clone(), image["height"].clone()),
            (json!("PNG"), json!(5), json!(2))
        );
        let data = STANDARD.decode(image["data"].as_str().expect("data")).expect("base64");
        assert_eq!(data, std::fs::read(repo.path().join("img.png")).expect("read"));
        assert_eq!((read["text"].clone(), read["bytes"].clone()), (Value::Null, json!(data.len())));
    }

    #[test]
    fn a_text_over_the_limit_says_its_size_and_sends_nothing_of_it() {
        let repo = project();
        with_files(&repo);
        let (mut app, rx) = started(&repo);

        let read = read(&mut app, &rx, "big.txt");

        assert_eq!((read["bytes"].clone(), read["text"].clone()), (json!(MAX_BYTES + 1), Value::Null));
    }

    #[test]
    fn a_link_to_a_device_answers_binary_and_does_not_wait_for_it() {
        let repo = project();
        symlink("/dev/zero", repo.path().join("zero")).expect("link");
        let (mut app, rx) = started(&repo);

        assert_eq!(read(&mut app, &rx, "zero")["kind"], "binary");
    }

    #[test]
    fn an_absolute_path_reads_without_a_project() {
        let home = TempDir::new();
        std::fs::write(home.path().join("a.txt"), "hello").expect("write");
        let (mut app, rx) = new_app(no_config());
        app.home = Some(home.path().to_path_buf());

        let read = read(&mut app, &rx, "~/a.txt");

        assert_eq!((read["text"].clone(), read["workspace"].clone()), (json!("hello"), Value::Null));
    }

    #[test]
    fn a_blank_path_is_malformed() {
        let repo = project();
        let (mut app, _rx) = started(&repo);

        assert_eq!(refused(&mut app, "files-read", &json!({"path": " "})).code, "malformed");
    }
}

mod following {
    use super::*;

    fn revision(line: &Value) -> u64 {
        line["revision"].as_u64().expect("a revision")
    }

    fn line_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, wanted: impl Fn(&Value) -> bool) -> Value {
        let mut found = None;
        wait_until(what, || {
            drain(app, rx);
            app.refresh(Instant::now());
            found = lines(app).into_iter().rfind(|line| wanted(line)).or(found.take());
            found.is_some()
        });
        found.expect("a line")
    }

    #[test]
    fn a_listing_answers_at_once_and_sends_a_line_when_a_file_appears() {
        let repo = project();
        let (mut app, rx) = started(&repo);

        let first = call_later(&mut app, &rx, "files-list", &json!({"follow": true})).expect("a listing");
        assert_eq!((first["stream"].clone(), revision(&first), first["boot"].is_string()), (json!(1), 1, true));
        std::fs::write(repo.path().join("later.txt"), "x").expect("write");

        let line = line_until(&mut app, &rx, "a second revision", |line| revision(line) >= 2);

        assert!(names(&line["folders"][0]).contains(&"later.txt".to_string()));
    }

    #[test]
    fn a_listing_that_did_not_change_sends_nothing() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        call_later(&mut app, &rx, "files-list", &json!({"follow": true})).expect("a listing");

        for _ in 0..5 {
            let event = rx.recv_timeout(Duration::from_secs(10)).expect("a feed ends");
            app.handle_event(event, AREA).expect("handled");
            app.refresh(Instant::now());
        }

        assert_eq!(lines(&mut app), Vec::<Value>::new());
    }

    #[test]
    fn a_read_sends_the_new_text() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        let first =
            call_later(&mut app, &rx, "files-read", &json!({"path": "notes.txt", "follow": true})).expect("a read");
        assert_eq!(first["text"], "one\nTWO\n");
        std::fs::write(repo.path().join("notes.tmp"), "one\nTWO\nthree\n").expect("write");
        std::fs::rename(repo.path().join("notes.tmp"), repo.path().join("notes.txt")).expect("replace");

        let line = line_until(&mut app, &rx, "the new text", |line| revision(line) >= 2);

        assert_eq!((line["text"].clone(), line["stream"].clone()), (json!("one\nTWO\nthree\n"), json!(1)));
    }

    #[test]
    fn one_connection_follows_one_stream_at_a_time() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        call_later(&mut app, &rx, "files-list", &json!({"follow": true})).expect("a listing");

        assert_eq!(refused(&mut app, "files-list", &json!({"follow": true})).code, "conflict");
        assert_eq!(refused(&mut app, "files-read", &json!({"path": "notes.txt", "follow": true})).code, "conflict");
    }

    #[test]
    fn it_ends_when_the_workspace_closes() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        call_later(&mut app, &rx, "files-list", &json!({"follow": true})).expect("a listing");
        let project = app.projects[0].id;
        ok(&mut app, "close", &json!({"item": {"project": project}}));

        app.refresh(Instant::now());

        assert!(streamed(&mut app).contains(&Streamed::End));
        assert!(app.clients.file_streams.is_empty());
    }

    #[test]
    fn a_client_that_leaves_before_the_answer_gets_none_and_leaves_no_stream() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        let text = json!({"contract": 1, "command": "files-list", "args": {}}).to_string();
        app.request(CLIENT, &text, Some(AREA), Instant::now());
        app.forget(CLIENT);

        let event = rx.recv_timeout(Duration::from_secs(10)).expect("the job ends");
        app.handle_event(event, AREA).expect("handled");

        assert_eq!(app.take_answers(), Vec::<(u64, String)>::new());
        assert!(app.clients.file_streams.is_empty());
    }
}

mod searching {
    use super::*;

    fn found(app: &mut App, rx: &Receiver<AppEvent>, args: &Value) -> Value {
        call_later(app, rx, "files-search", args).unwrap_or_else(|e| panic!("{}", e.message))
    }

    #[test]
    fn names_are_ranked_smart_cased_and_say_which_characters_matched() {
        let repo = git_repo(&[("src/Main.rs", "x\n"), ("src/other.rs", "x\n"), ("docs/main.md", "x\n")]);
        let (mut app, rx) = started(&repo);

        let lower = found(&mut app, &rx, &json!({"query": "main", "mode": "names"}));
        let upper = found(&mut app, &rx, &json!({"query": "Main", "mode": "names"}));

        let paths = |found: &Value| -> Vec<String> {
            found["names"]
                .as_array()
                .expect("names")
                .iter()
                .map(|n| n["path"].as_str().expect("path").to_string())
                .collect()
        };
        assert_eq!(paths(&lower).len(), 2);
        assert_eq!(paths(&upper), ["src/Main.rs"]);
        assert_eq!(upper["names"][0]["indices"], json!([4, 5, 6, 7]));
        assert_eq!(
            (upper["mode"].clone(), upper["capped"].clone(), upper["workspace"].clone()),
            (json!("names"), json!(false), json!(workspace_id(&app)))
        );
    }

    #[test]
    fn text_has_the_matching_lines_and_where_in_them() {
        let repo = git_repo(&[("src/a.rs", "one\nlet needle = 1;\n"), ("b.txt", "nothing\n")]);
        std::fs::write(repo.path().join("ignored.log"), "needle\n").expect("write");
        let (mut app, rx) = started(&repo);

        let found = found(&mut app, &rx, &json!({"query": "needle"}));

        assert_eq!(
            (found["mode"].clone(), found["matches"].clone(), found["capped"].clone()),
            (json!("text"), json!(2), json!(false))
        );
        let files = found["files"].as_array().expect("files");
        let first = files.iter().find(|f| f["path"] == "src/a.rs").expect("src/a.rs");
        assert_eq!(first["lines"][0], json!({"number": 2, "text": "let needle = 1;", "ranges": [[4, 10]]}));
    }

    #[test]
    fn a_blank_query_is_malformed_and_a_relative_search_needs_a_workspace() {
        let repo = project();
        let (mut app, _rx) = started(&repo);
        assert_eq!(refused(&mut app, "files-search", &json!({"query": "  "})).code, "malformed");

        let (mut app, _rx) = new_app(no_config());
        assert!(
            refused(&mut app, "files-search", &json!({"query": "a"})).message.contains("no workspace is open here")
        );
    }

    #[test]
    fn a_client_that_leaves_cancels_its_search() {
        let repo = project();
        let (mut app, rx) = started(&repo);
        let text = json!({"contract": 1, "command": "files-search", "args": {"query": "one"}}).to_string();
        app.request(CLIENT, &text, Some(AREA), Instant::now());
        assert!(!app.clients.file_streams.is_empty());
        app.forget(CLIENT);
        assert!(app.clients.file_streams.is_empty());

        let event = rx.recv_timeout(Duration::from_secs(10)).expect("the job ends");
        app.handle_event(event, AREA).expect("handled");
        assert_eq!(app.take_answers(), Vec::<(u64, String)>::new());
    }

    #[test]
    fn a_newer_search_of_the_same_connection_cancels_the_earlier_one() {
        let mut streams = crate::app::files_panel::Streams::default();

        let first = streams.search(CLIENT, 1);
        let other = streams.search(CLIENT + 1, 2);
        let second = streams.search(CLIENT, 3);

        assert_eq!(
            (streams.cancelled(1), streams.cancelled(2), streams.cancelled(3)),
            (Some(true), Some(false), Some(false))
        );
        assert!(first.load(std::sync::atomic::Ordering::Relaxed));
        assert!(
            !other.load(std::sync::atomic::Ordering::Relaxed) && !second.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
}

mod the_server_s_home {
    use super::*;

    #[test]
    fn open_takes_a_folder_in_the_home_folder() {
        let home = TempDir::new();
        std::fs::create_dir(home.path().join("proj")).expect("mkdir");
        let (mut app, _rx) = new_app(no_config());
        app.home = Some(home.path().to_path_buf());

        ok(&mut app, "open", &json!({"path": "~/proj"}));

        assert_eq!(app.projects[0].path, home.path().join("proj"));
    }

    #[test]
    fn open_still_says_the_path_as_given_when_it_is_missing() {
        let home = TempDir::new();
        let (mut app, _rx) = new_app(no_config());
        app.home = Some(home.path().to_path_buf());

        let refused = refused(&mut app, "open", &json!({"path": "~/nope"}));

        assert!(refused.message.contains("cannot open `~/nope`"), "{}", refused.message);
    }

    #[test]
    fn status_names_it() {
        let home = TempDir::new();
        let (mut app, _rx) = new_app(no_config());
        app.home = Some(home.path().to_path_buf());

        let status = ok(&mut app, "status", &json!({}));

        assert_eq!(status["home"], home.path().display().to_string());
    }
}
