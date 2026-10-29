use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, git_repo};

mod folder_name {
    use super::*;

    #[rstest]
    #[case::https("https://github.com/acme/shop.git", Some("shop"))]
    #[case::https_without_suffix("https://github.com/acme/shop", Some("shop"))]
    #[case::scp_like("git@github.com:acme/shop.git", Some("shop"))]
    #[case::trailing_slash("/srv/git/shop/", Some("shop"))]
    #[case::dot_git_folder("/srv/git/shop/.git", Some("shop"))]
    #[case::bundle("/tmp/shop.bundle", Some("shop"))]
    #[case::empty("", None)]
    #[case::only_slashes("///", None)]
    #[case::dots("/srv/..", None)]
    #[case::scheme_only("https://", None)]
    #[case::host_only("https://example.com/", Some("example.com"))]
    fn is_the_repositorys_last_part(#[case] url: &str, #[case] name: Option<&str>) {
        assert_eq!(folder_name(url).as_deref(), name);
    }
}

mod failure {
    use super::*;

    const URL: &str = "git@example.com:acme/shop.git";

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
    fn says_it_is_not_a_repository(#[case] stderr: &str, #[case] detail: &str) {
        assert_eq!(failure(URL, stderr), Failure::NotRepository { url: URL.into(), detail: detail.into() });
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
    fn says_access_was_denied(#[case] stderr: &str, #[case] detail: &str) {
        assert_eq!(failure(URL, stderr), Failure::AccessDenied { url: URL.into(), detail: detail.into() });
    }

    #[test]
    fn otherwise_gives_gits_first_reason() {
        let stderr = "ssh: connect to host example.com port 22: Connection refused\nfatal: Could not read from \
                      remote repository.\n\nPlease make sure you have the correct access rights\n";

        assert_eq!(
            failure(URL, stderr).to_string(),
            "git clone failed: ssh: connect to host example.com port 22: Connection refused"
        );
    }
}

mod cloning {
    use super::*;

    #[test]
    fn checks_the_repository_out_in_a_folder_named_after_it() {
        let repo = git_repo(&[("README.md", "hi\n")]);
        let parent = TempDir::new();

        let target = target(&repo.path().display().to_string(), parent.path()).expect("a target");
        run(&target).expect("cloned");

        let name = repo.path().file_name().expect("a name");
        assert_eq!(target.path, parent.path().join(name));
        assert_eq!(std::fs::read_to_string(target.path.join("README.md")).expect("the file"), "hi\n");
    }

    #[test]
    fn takes_an_empty_folder_and_refuses_one_with_files() {
        let repo = git_repo(&[]);
        let parent = TempDir::new();
        let path = parent.path().join(repo.path().file_name().expect("a name"));
        std::fs::create_dir(&path).expect("mkdir");
        let url = repo.path().display().to_string();

        assert!(target(&url, parent.path()).is_ok());
        std::fs::write(path.join("notes.txt"), "mine").expect("write");
        assert_eq!(target(&url, parent.path()), Err(Failure::NotEmpty(path)));
    }

    #[test]
    fn that_fails_removes_the_folders_it_made() {
        let parent = TempDir::new();
        let missing = parent.path().join("missing");
        let url = format!("file://{}", missing.display());

        let target = target(&url, &parent.path().join("new").join("deeper")).expect("a target");
        let failed = run(&target);

        assert!(matches!(failed, Err(Failure::NotRepository { .. })), "{failed:?}");
        assert!(!parent.path().join("new").exists(), "the folders it made are gone");
    }

    #[test]
    fn that_fails_keeps_the_empty_folder_it_was_given() {
        let parent = TempDir::new();
        let path = parent.path().join("missing");
        std::fs::create_dir(&path).expect("mkdir");
        let url = format!("file://{}", parent.path().join("elsewhere").join("missing").display());

        let target = target(&url, parent.path()).expect("a target");
        let failed = run(&target);

        assert!(failed.is_err());
        assert!(path.is_dir(), "the folder that was there stays");
    }
}
