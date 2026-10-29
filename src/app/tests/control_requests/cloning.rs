use super::*;

fn clone_args(url: &str, parent: &Path) -> Value {
    json!({"url": url, "parent": parent})
}

fn answered_at_last(app: &mut App, rx: &Receiver<AppEvent>) -> Vec<(u64, String)> {
    let mut got = Vec::new();
    wait_until("the clone's answer", || {
        drain(app, rx);
        got.extend(app.take_answers());
        !got.is_empty()
    });
    got
}

mod a_clone {
    use super::*;

    #[test]
    fn opens_the_repository_as_a_project_and_answers_its_path() {
        let repo = git_repo(&[("README.md", "hi\n")]);
        let parent = TempDir::new();
        let (mut app, rx) = empty_app();

        let answer = asked(&mut app, &rx, "clone", &clone_args(&repo.path().display().to_string(), parent.path()))
            .expect("cloned");

        let path = parent.path().join(repo.path().file_name().expect("a name"));
        assert_eq!(answer["path"], json!(path));
        assert_eq!((answer["project"].clone(), app.projects[0].path.clone()), (json!(app.projects[0].id), path));
        assert!(app.projects[0].path.join("README.md").is_file());
    }

    #[test]
    fn of_something_that_is_not_a_repository_is_not_found_and_leaves_no_folder() {
        let parent = TempDir::new();
        let url = format!("file://{}", parent.path().join("nothing").display());
        let (mut app, rx) = empty_app();

        let refused =
            asked(&mut app, &rx, "clone", &clone_args(&url, &parent.path().join("src"))).expect_err("refused");

        assert_eq!(refused.code, "not-found");
        assert!(refused.message.contains("is not a git repository"), "{}", refused.message);
        assert!(!parent.path().join("src").exists(), "no folder is left");
        assert!(app.projects.is_empty());
    }

    #[test]
    fn into_a_folder_that_is_already_being_cloned_into_is_busy() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let (mut app, rx) = empty_app();
        let args = clone_args(&repo.path().display().to_string(), parent.path());
        let text = json!({"contract": 1, "command": "clone", "args": args}).to_string();
        app.request(CLIENT + 1, &text, Some(AREA), Instant::now());

        let busy = refused(&mut app, "clone", &args);

        assert_eq!(busy.code, "busy");
        assert_eq!(answered_at_last(&mut app, &rx).len(), 1);
        assert_eq!(app.projects.len(), 1);
    }
}

mod refused_at_once {
    use super::*;

    #[test]
    fn a_folder_that_is_there_and_not_empty() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let taken = parent.path().join(repo.path().file_name().expect("a name"));
        std::fs::create_dir(&taken).expect("mkdir");
        std::fs::write(taken.join("notes.txt"), "mine").expect("write");
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &clone_args(&repo.path().display().to_string(), parent.path()));

        assert_eq!(refused.code, "conflict");
        assert!(refused.message.contains("already exists and is not an empty folder"), "{}", refused.message);
        assert_eq!(std::fs::read_to_string(taken.join("notes.txt")).expect("kept"), "mine");
    }

    #[test]
    fn a_relative_folder() {
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &json!({"url": "/srv/shop.git", "parent": "src"}));

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("must be absolute"), "{}", refused.message);
    }

    #[test]
    fn a_url_with_no_name_in_it() {
        let parent = TempDir::new();
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &clone_args("https://", parent.path()));

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("cannot tell the folder's name"), "{}", refused.message);
    }

    #[test]
    fn a_folder_in_the_home_folder_is_the_servers() {
        let home = TempDir::new();
        std::fs::create_dir(home.path().join("src")).expect("mkdir");
        std::fs::write(home.path().join("src").join("shop"), "a file").expect("write");
        let (mut app, _rx) = empty_app();
        app.home = Some(home.path().to_path_buf());

        let refused = refused(&mut app, "clone", &json!({"url": "/srv/shop.git", "parent": "~/src"}));

        assert_eq!(refused.code, "conflict");
        assert!(refused.message.contains(&home.path().join("src").join("shop").display().to_string()));
    }
}
