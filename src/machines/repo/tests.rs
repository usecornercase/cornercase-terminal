use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use rstest::rstest;

use super::*;
use crate::test_util::{self, TempDir, git_repo, write_executable};

fn counted(hostname: Option<&'static str>) -> (Resolve, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let resolve: Resolve = Arc::new(move |host: &str| {
        seen.lock().push(host.to_string());
        hostname.map(str::to_string)
    });
    (resolve, asked)
}

fn repo_with(config: &str) -> TempDir {
    let repo = git_repo(&[]);
    std::fs::write(repo.path().join(".git/config"), config).expect("write config");
    repo
}

mod hostnames {
    use super::*;

    #[test]
    fn resolve_each_host_once_for_every_project() {
        let (resolve, asked) = counted(Some("github.com"));
        let hostnames = Hostnames::new(resolve);
        let shop = repo_with("[remote \"origin\"]\n\turl = gh-work:acme/web-shop.git\n");
        let blog = repo_with("[remote \"origin\"]\n\turl = gh-work:acme/blog\n");

        assert_eq!(hostnames.repo(shop.path()).identity.as_deref(), Some("github.com/acme/web-shop"));
        assert_eq!(hostnames.clone().repo(blog.path()).identity.as_deref(), Some("github.com/acme/blog"));
        assert_eq!(*asked.lock(), ["gh-work"]);
    }

    #[test]
    fn keep_the_host_when_it_cannot_be_resolved_and_do_not_ask_again() {
        let (resolve, asked) = counted(None);
        let hostnames = Hostnames::new(resolve);

        assert_eq!(hostnames.of("box"), "box");
        assert_eq!(hostnames.of("box"), "box");
        assert_eq!(*asked.lock(), ["box"]);
    }

    #[test]
    fn resolve_a_host_once_when_asked_at_the_same_time() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let (release, held) = mpsc::channel::<()>();
        let held = Mutex::new(held);
        let hostnames = Hostnames::new(Arc::new(move |_: &str| {
            counter.fetch_add(1, Ordering::SeqCst);
            let _ = held.lock().recv();
            Some("github.com".into())
        }));
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let hostnames = hostnames.clone();
                std::thread::spawn(move || hostnames.of("gh-work"))
            })
            .collect();
        let waiting = || hostnames.known.lock().get("gh-work").map_or(0, Arc::strong_count);

        test_util::wait_until("every thread waits for the one lookup", || {
            calls.load(Ordering::SeqCst) == 1 && waiting() == 5
        });
        drop(release);

        for thread in threads {
            assert_eq!(thread.join().expect("join"), "github.com");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn leave_out_a_remote_s_query_and_fragment() {
        let (resolve, _) = counted(None);
        let hostnames = Hostnames::new(resolve);
        let config = "[remote \"origin\"]\n\turl = https://github.com/acme/web-shop.git?access_token=ghp_abc123#main\n";

        let repo = hostnames.repo(repo_with(config).path());

        assert_eq!(repo.identity.as_deref(), Some("github.com/acme/web-shop"));
        assert_eq!(repo.remotes, [("origin".to_string(), "https://github.com/acme/web-shop.git".to_string())]);
    }

    #[test]
    fn read_the_remotes_and_no_identity_without_them() {
        let (resolve, asked) = counted(Some("github.com"));
        let hostnames = Hostnames::new(resolve);
        let config = "[remote \"origin\"]\n\turl = https://github.com/me/web-shop\n\
                      [remote \"upstream\"]\n\turl = https://github.com/acme/web-shop\n";

        let repo = hostnames.repo(repo_with(config).path());

        assert_eq!(repo.identity.as_deref(), Some("github.com/acme/web-shop"));
        assert_eq!(
            repo.remotes,
            [
                ("origin".to_string(), "https://github.com/me/web-shop".to_string()),
                ("upstream".to_string(), "https://github.com/acme/web-shop".to_string()),
            ]
        );
        assert_eq!(hostnames.repo(git_repo(&[]).path()), Repo::default());
        assert!(asked.lock().is_empty());
    }

    #[test]
    fn keep_no_credentials_from_the_remote_urls() {
        let (resolve, _) = counted(None);
        let config = "[remote \"origin\"]\n\turl = https://ana:ghp_s3cret@github.com/acme/web-shop.git\n";

        let repo = Hostnames::new(resolve).repo(repo_with(config).path());

        assert_eq!(repo.remotes, [("origin".to_string(), "https://github.com/acme/web-shop.git".to_string())]);
        assert_eq!(repo.identity.as_deref(), Some("github.com/acme/web-shop"));
    }
}

mod ssh_hostname {
    use super::*;

    fn fake_ssh(dir: &TempDir, script: &str) -> String {
        let path = dir.path().join("ssh");
        write_executable(
            &path,
            &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}/args\"\n{script}", dir.path().display()),
        );
        path.display().to_string()
    }

    #[test]
    fn is_the_hostname_ssh_g_prints() {
        let dir = TempDir::new();
        let ssh = fake_ssh(&dir, "printf 'user git\\nhostname GitHub.com\\nport 22\\n'\n");

        assert_eq!(hostname(&ssh, "gh-work").as_deref(), Some("github.com"));
        assert_eq!(std::fs::read_to_string(dir.path().join("args")).expect("read args"), "-G\n--\ngh-work\n");
    }

    #[test]
    fn is_none_when_ssh_fails_or_is_missing() {
        let dir = TempDir::new();
        let ssh = fake_ssh(&dir, "printf 'hostname github.com\\n'\nexit 255\n");

        assert_eq!(hostname(&ssh, "gh-work"), None);
        assert_eq!(hostname("/nonexistent/ssh", "gh-work"), None);
    }

    #[test]
    fn never_runs_ssh_for_a_host_that_looks_like_an_option_or_holds_spaces() {
        let dir = TempDir::new();
        let ssh = fake_ssh(&dir, "printf 'hostname github.com\\n'\n");

        assert_eq!(hostname(&ssh, "-oProxyCommand=touch"), None);
        assert_eq!(hostname(&ssh, "gh work"), None);
        assert!(!dir.path().join("args").exists());
    }

    #[rstest]
    #[case::a_space("hostname evil host\n")]
    #[case::a_shell_character("hostname github.com;rm\n")]
    #[case::an_escape("hostname git\u{1b}[31mhub.com\n")]
    #[case::a_slash("hostname github.com/acme\n")]
    #[case::an_at("hostname git@github.com\n")]
    fn is_none_when_the_hostname_is_not_a_plain_name(#[case] config: &str) {
        assert_eq!(configured(config), None);
    }

    #[test]
    fn keeps_dots_dashes_and_underscores() {
        assert_eq!(configured("hostname Git_Hub-1.example.com\n").as_deref(), Some("git_hub-1.example.com"));
    }

    #[test]
    fn keeps_the_host_as_it_is_when_ssh_prints_an_odd_hostname() {
        let dir = TempDir::new();
        let ssh = fake_ssh(&dir, "printf 'hostname evil;host\\n'\n");
        let hostnames = Hostnames::new(Arc::new(move |host: &str| hostname(&ssh, host)));

        assert_eq!(hostnames.of("gh-work"), "gh-work");
    }

    #[test]
    fn is_none_without_a_hostname_line() {
        assert_eq!(configured("user git\nport 22\n"), None);
        assert_eq!(configured("hostname \n"), None);
    }
}

mod pairing {
    use super::*;

    const SHOP: Option<&str> = Some("github.com/acme/web-shop");
    const BLOG: Option<&str> = Some("github.com/acme/blog");

    #[rstest]
    #[case::the_first_of_two_local_projects_with_the_same_identity(
        &[SHOP],
        &[(7, BLOG), (3, SHOP), (5, SHOP)],
        &[Some(3)]
    )]
    #[case::two_remote_projects_with_one_identity_go_to_the_same_local_one(
        &[SHOP, BLOG, SHOP],
        &[(3, SHOP), (7, BLOG)],
        &[Some(3), Some(7), Some(3)]
    )]
    #[case::a_remote_project_without_an_identity_stays_on_its_own(&[None], &[(3, SHOP), (4, None)], &[None])]
    #[case::a_local_project_without_an_identity_takes_none(&[SHOP], &[(4, None)], &[None])]
    #[case::an_identity_no_local_project_has_stays_on_its_own(&[BLOG], &[(3, SHOP)], &[None])]
    #[case::identities_match_whatever_their_case(&[Some("GitHub.com/Acme/Web-Shop")], &[(3, SHOP)], &[Some(3)])]
    fn give_each_remote_project_the_first_local_one_with_its_identity(
        #[case] theirs: &[Option<&str>],
        #[case] ours: &[(u64, Option<&str>)],
        #[case] paired: &[Option<u64>],
    ) {
        assert_eq!(pairs(theirs, ours), paired);
    }
}
