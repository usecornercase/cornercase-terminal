use rstest::rstest;

use super::*;
use crate::test_util::FakeHttp;

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

fn start_clone(app: &mut App, client: u64, args: &Value) {
    let text = json!({"contract": 1, "command": "clone", "args": args}).to_string();
    app.request(client, &text, Some(AREA), Instant::now());
}

mod a_clone {
    use super::*;

    #[test]
    fn opens_the_repository_as_a_saved_project_and_answers_its_path() {
        let repo = git_repo(&[("README.md", "hi\n")]);
        let parent = TempDir::new();
        let (mut app, rx) = empty_app();

        let answer = asked(&mut app, &rx, "clone", &clone_args(&repo.path().display().to_string(), parent.path()))
            .expect("cloned");

        let path = parent.path().join(repo.path().file_name().expect("a name"));
        assert_eq!(answer["path"], json!(path));
        assert_eq!(
            (answer["project"].clone(), app.projects[0].path.clone()),
            (json!(app.projects[0].id), path.clone())
        );
        assert!(app.projects[0].path.join("README.md").is_file());
        assert!(app.state().projects.iter().any(|saved| saved.path == path), "the session saves it");
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
    fn that_fails_says_why_without_the_token_in_its_url() {
        let http = FakeHttp::start(vec![("GET /acme/shop.git/info/refs", 404, "")]);
        let url = http.url().replace("http://", "http://ana:secret-token@") + "/acme/shop.git";
        let parent = TempDir::new();
        let (mut app, rx) = empty_app();

        let refused = asked(&mut app, &rx, "clone", &clone_args(&url, parent.path())).expect_err("refused");

        assert_eq!(refused.code, "not-found", "{}", refused.message);
        assert!(refused.message.contains("is not a git repository"), "{}", refused.message);
        assert!(!refused.message.contains("secret-token"), "{}", refused.message);
        assert!(!parent.path().join("shop").exists());
    }

    #[test]
    fn into_a_folder_that_is_already_being_cloned_into_is_busy_under_any_spelling() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let link = TempDir::new();
        let spelled = link.path().join("same");
        std::os::unix::fs::symlink(parent.path(), &spelled).expect("link");
        let (mut app, rx) = empty_app();
        let url = repo.path().display().to_string();
        start_clone(&mut app, CLIENT + 1, &clone_args(&url, parent.path()));
        let folder = parent.path().join(repo.path().file_name().expect("a name"));
        std::fs::write(folder.join("started"), "git is at work").expect("write");

        let busy = refused(&mut app, "clone", &clone_args(&url, &spelled));

        assert_eq!(busy.code, "busy");
        assert_eq!(answered_at_last(&mut app, &rx).len(), 1);
    }

    #[test]
    fn whose_command_left_still_opens_its_project() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let (mut app, rx) = empty_app();
        start_clone(&mut app, CLIENT, &clone_args(&repo.path().display().to_string(), parent.path()));

        app.forget(CLIENT);

        wait_until("the project opens", || {
            drain(&mut app, &rx);
            !app.projects.is_empty()
        });
        assert_eq!(app.take_answers(), Vec::new());
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
    fn a_folder_that_goes_up_and_leaves_what_is_there_alone() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let kept = parent.path().join("kept").join(repo.path().file_name().expect("a name"));
        std::fs::create_dir_all(&kept).expect("mkdir");
        std::fs::write(kept.join("work.txt"), "mine").expect("write");
        let up = parent.path().join("missing").join("..").join("kept");
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &clone_args(&repo.path().display().to_string(), &up));

        assert_eq!(refused.code, "malformed");
        assert_eq!(std::fs::read_to_string(kept.join("work.txt")).expect("kept"), "mine");
        assert!(!parent.path().join("missing").exists());
    }

    #[test]
    fn a_relative_folder() {
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &json!({"url": "/srv/shop.git", "parent": "src"}));

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains("must be absolute"), "{}", refused.message);
    }

    #[test]
    fn a_relative_path_to_a_repository() {
        let parent = TempDir::new();
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &clone_args("../repos/shop", parent.path()));

        assert_eq!(refused.code, "malformed");
        assert!(!parent.path().join("shop").exists());
    }

    #[rstest]
    #[case::no_name("https://user:secret@example.com/..", "cannot tell the folder's name")]
    #[case::git_folder("https://user:secret@example.com/x/.git.git", "cannot be a clone's folder")]
    fn a_url_without_a_usable_name_and_without_quoting_it(#[case] url: &str, #[case] says: &str) {
        let parent = TempDir::new();
        let (mut app, _rx) = empty_app();

        let refused = refused(&mut app, "clone", &clone_args(url, parent.path()));

        assert_eq!(refused.code, "malformed");
        assert!(refused.message.contains(says) && !refused.message.contains("secret"), "{}", refused.message);
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
