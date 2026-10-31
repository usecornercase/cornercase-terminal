use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, git_repo};

fn prepared(url: &str, parent: &Path) -> Target {
    let path = destination(url, parent).expect("a destination");
    prepare(url, path).expect("a target")
}

mod folder_name {
    use super::*;

    #[rstest]
    #[case::https("https://github.com/acme/shop.git", "shop")]
    #[case::https_without_suffix("https://github.com/acme/shop", "shop")]
    #[case::scp_like("git@github.com:acme/shop.git", "shop")]
    #[case::trailing_slash("/srv/git/shop/", "shop")]
    #[case::dot_git_folder("/srv/git/shop/.git", "shop")]
    #[case::bundle("/tmp/shop.bundle", "shop")]
    #[case::host_only("https://example.com/", "example.com")]
    #[case::query("https://example.com/acme/shop.git?token=secret", "shop")]
    #[case::fragment("https://example.com/acme/shop.git#main", "shop")]
    #[case::query_after_a_slash("https://example.com/acme/shop/?ref=main#top", "shop")]
    #[case::scp_like_query("git@github.com:acme/shop.git?token=secret", "shop")]
    #[case::local_path_with_a_hash("/srv/git/shop#2", "shop#2")]
    #[case::credentials_without_a_path("https://user:secret@example.com", "example.com")]
    #[case::credentials_with_a_hash("https://user:ab#secret@example.com/acme/shop.git", "shop")]
    #[case::credentials_with_a_question_mark("https://user:ab?secret@example.com/", "example.com")]
    #[case::query_with_a_slash_without_a_path("https://example.com?token=ab/cd", "example.com")]
    fn is_the_repositorys_last_part(#[case] url: &str, #[case] name: &str) {
        assert_eq!(folder_name(url).as_deref(), Ok(name));
    }

    #[rstest]
    #[case::empty("")]
    #[case::only_slashes("///")]
    #[case::dots("/srv/..")]
    #[case::scheme_only("https://")]
    fn is_refused_when_there_is_none(#[case] url: &str) {
        assert_eq!(folder_name(url), Err(Failure::NoName));
    }

    #[rstest]
    #[case::git_git("https://example.com/x/.git.git")]
    #[case::any_case("https://example.com/x/.GIT.git")]
    #[case::bundle("/tmp/.git.bundle")]
    fn is_refused_when_it_would_be_a_git_folder(#[case] url: &str) {
        assert_eq!(folder_name(url), Err(Failure::GitFolder));
    }
}

mod is_local_path {
    use super::*;

    #[rstest]
    #[case::absolute("/srv/git/shop", true)]
    #[case::relative("./shop", true)]
    #[case::bare("shop", true)]
    #[case::colon_after_a_slash("./a:b", true)]
    #[case::https("https://github.com/acme/shop", false)]
    #[case::file("file:///srv/git/shop", false)]
    #[case::scp_like("git@github.com:acme/shop.git", false)]
    #[case::scp_like_without_a_slash("host:shop", false)]
    fn tells_a_path_from_a_url(#[case] url: &str, #[case] local: bool) {
        assert_eq!(is_local_path(url), local);
    }
}

mod failure {
    use super::*;

    #[rstest]
    #[case::missing_local("fatal: repository '/srv/shop' does not exist\n", "repository '/srv/shop' does not exist")]
    #[case::github_https(
        "remote: Repository not found.\nfatal: repository 'https://github.com/acme/shop/' not found\n",
        "Repository not found"
    )]
    #[case::not_a_repository(
        "fatal: '/srv/shop' does not appear to be a git repository\nfatal: Could not read from remote repository.\n",
        "'/srv/shop' does not appear to be a git repository"
    )]
    #[case::gitlab(
        "remote: \nremote: ========\nremote: \nremote: The project you were looking for could not be found or you \
         don't have permission to view it.\n",
        "The project you were looking for could not be found or you don't have permission to view it"
    )]
    fn says_it_is_not_a_repository(#[case] stderr: &str, #[case] detail: &str) {
        assert_eq!(failure(stderr), Failure::NotRepository(detail.into()));
    }

    #[rstest]
    #[case::ssh_key(
        "git@example.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n\nPlease make \
         sure you have the correct access rights\nand the repository exists.\n",
        "git@example.com: Permission denied (publickey)"
    )]
    #[case::no_prompt(
        "fatal: could not read Username for 'https://example.com': terminal prompts disabled\n",
        "could not read Username for 'https://example.com': terminal prompts disabled"
    )]
    #[case::bad_password(
        "fatal: Authentication failed for 'https://example.com/acme/shop.git/'\n",
        "Authentication failed for 'https://example.com/acme/shop.git/'"
    )]
    #[case::unauthorized(
        "fatal: unable to access 'https://example.com/shop.git/': The requested URL returned error: 401\n",
        "unable to access 'https://example.com/shop.git/': The requested URL returned error: 401"
    )]
    fn says_access_was_denied(#[case] stderr: &str, #[case] detail: &str) {
        assert_eq!(failure(stderr), Failure::AccessDenied(detail.into()));
    }

    #[test]
    fn otherwise_gives_gits_first_reason() {
        let stderr = "ssh: connect to host example.com port 22: Connection refused\nfatal: Could not read from \
                      remote repository.\n\nPlease make sure you have the correct access rights\n";

        assert_eq!(
            failure(stderr).to_string(),
            "git clone failed: ssh: connect to host example.com port 22: Connection refused"
        );
    }

    #[test]
    fn reads_no_warning() {
        let stderr = "warning: templates not found in /usr/share/git-core/templates\nfatal: unable to access \
                      'https://example.com/shop.git/': Failed to connect to example.com port 443\n";

        assert!(matches!(failure(stderr), Failure::Failed(_)), "{:?}", failure(stderr));
    }

    #[rstest]
    #[case::not_found(
        "fatal: repository 'http://127.0.0.1:9/acme/shop.git?token=secret/' not found\n",
        Failure::NotRepository("repository 'http://127.0.0.1:9/acme/shop.git' not found".into())
    )]
    #[case::unreachable(
        "fatal: unable to access 'http://127.0.0.1:9/acme/shop.git?token=secret/': Failed to connect to 127.0.0.1 \
         port 9 after 0 ms: Couldn't connect to server\n",
        Failure::Failed(
            "unable to access 'http://127.0.0.1:9/acme/shop.git': Failed to connect to 127.0.0.1 port 9 after 0 ms: \
             Couldn't connect to server"
                .into()
        )
    )]
    #[case::query_with_a_slash_without_a_path(
        "fatal: repository 'https://example.com?token=ab/cd/' not found\n",
        Failure::NotRepository("repository 'https://example.com' not found".into())
    )]
    #[case::fragment(
        "fatal: unable to access 'https://example.com/shop.git#secret/': Could not resolve host: example.com\n",
        Failure::Failed("unable to access 'https://example.com/shop.git': Could not resolve host: example.com".into())
    )]
    fn never_quotes_the_query_or_the_fragment_of_a_url(#[case] stderr: &str, #[case] expected: Failure) {
        assert_eq!(failure(stderr), expected);
    }

    #[test]
    fn never_quotes_a_password_or_a_token() {
        let stderr = "fatal: unable to access 'https://ana:hunter2@example.com/shop.git/': Could not resolve host\n";

        let said = failure(stderr).to_string();

        assert!(!said.contains("hunter2") && !said.contains("ana"), "{said}");
        assert!(said.contains("'https://example.com/shop.git/'"), "{said}");
    }
}

mod destination {
    use super::*;

    #[test]
    fn follows_links_in_the_folders_that_exist() {
        let temp = TempDir::new();
        std::fs::create_dir(temp.path().join("real")).expect("mkdir");
        std::os::unix::fs::symlink(temp.path().join("real"), temp.path().join("link")).expect("link");

        let path = destination("/srv/shop.git", &temp.path().join("link").join("new")).expect("a destination");

        assert_eq!(path, temp.path().join("real").join("new").join("shop"));
    }

    #[test]
    fn follows_a_link_that_is_the_clones_own_folder() {
        let temp = TempDir::new();
        std::fs::create_dir(temp.path().join("empty")).expect("mkdir");
        std::os::unix::fs::symlink(temp.path().join("empty"), temp.path().join("shop")).expect("link");

        let path = destination("/srv/shop.git", temp.path()).expect("a destination");

        assert_eq!(path, temp.path().join("empty"));
    }
}

mod cloning {
    use super::*;

    #[test]
    fn checks_the_repository_out_in_a_folder_named_after_it() {
        let repo = git_repo(&[("README.md", "hi\n")]);
        let parent = TempDir::new();

        let target = prepared(&repo.path().display().to_string(), parent.path());
        run(&target).expect("cloned");

        assert_eq!(target.path, parent.path().join(repo.path().file_name().expect("a name")));
        assert_eq!(std::fs::read_to_string(target.path.join("README.md")).expect("the file"), "hi\n");
    }

    #[test]
    fn takes_an_empty_folder_and_refuses_one_with_files() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let path = parent.path().join(repo.path().file_name().expect("a name"));
        std::fs::create_dir(&path).expect("mkdir");
        let url = repo.path().display().to_string();

        assert!(prepare(&url, path.clone()).is_ok());
        std::fs::write(path.join("notes.txt"), "mine").expect("write");
        assert_eq!(prepare(&url, path.clone()), Err(Failure::NotEmpty(path)));
    }

    #[test]
    fn that_fails_removes_the_folders_it_made() {
        let parent = TempDir::new();
        let url = format!("file://{}", parent.path().join("missing").display());

        let target = prepared(&url, &parent.path().join("new").join("deeper"));
        let failed = run(&target);

        assert!(matches!(failed, Err(Failure::NotRepository(_))), "{failed:?}");
        assert!(!parent.path().join("new").exists(), "the folders it made are gone");
    }

    #[test]
    fn that_fails_keeps_a_folder_it_made_once_something_else_is_in_it() {
        let parent = TempDir::new();
        let url = format!("file://{}", parent.path().join("missing").display());
        let target = prepared(&url, &parent.path().join("new"));
        std::fs::write(parent.path().join("new").join("notes.txt"), "mine").expect("write");

        let failed = run(&target);

        assert!(matches!(failed, Err(Failure::NotRepository(_))), "{failed:?}");
        assert!(!target.path.exists(), "the clone's folder is gone");
        assert_eq!(std::fs::read_to_string(parent.path().join("new").join("notes.txt")).expect("kept"), "mine");
    }

    #[test]
    fn that_fails_keeps_the_empty_folder_it_was_given() {
        let parent = TempDir::new();
        let path = parent.path().join("missing");
        std::fs::create_dir(&path).expect("mkdir");
        let url = format!("file://{}", parent.path().join("elsewhere").join("missing").display());

        let failed = run(&prepared(&url, parent.path()));

        assert!(failed.is_err());
        assert!(path.is_dir(), "the folder that was there stays");
    }

    #[test]
    fn that_fails_into_a_folder_another_clone_made_leaves_that_clone_alone() {
        let repo = git_repo(&[("README.md", "hi\n")]);
        let parent = TempDir::new();
        let url = repo.path().display().to_string();
        let first = prepared(&url, parent.path());
        let second = prepared(&url, parent.path());

        run(&first).expect("the first clone");
        let failed = run(&second);

        assert!(failed.is_err());
        assert!(first.path.join("README.md").is_file(), "the first clone is still there");
    }

    #[test]
    fn that_cannot_make_its_folder_says_so_and_leaves_nothing() {
        let parent = TempDir::new();
        std::fs::write(parent.path().join("file"), "not a folder").expect("write");

        let refused = prepare("/srv/shop.git", parent.path().join("file").join("sub").join("shop"));

        assert!(matches!(refused, Err(Failure::Folder(_))), "{refused:?}");
        assert_eq!(std::fs::read_to_string(parent.path().join("file")).expect("kept"), "not a folder");
    }
}
