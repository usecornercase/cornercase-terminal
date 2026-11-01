use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn repo_with_head(head: &str) -> TempDir {
    let tmp = TempDir::new();
    std::fs::create_dir(tmp.path().join(".git")).expect("create .git");
    std::fs::write(tmp.path().join(".git/HEAD"), head).expect("write HEAD");
    tmp
}

mod main_worktree {
    use super::*;

    #[test]
    fn is_the_folder_holding_the_git_dir() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let sub = repo.path().join("src");
        std::fs::create_dir(&sub).expect("create subfolder");
        assert_eq!(main_worktree(&sub), Some(repo.path().to_path_buf()));
    }

    #[test]
    fn is_none_inside_a_linked_worktree() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join(".git"), "gitdir: /repo/.git/worktrees/wt\n").expect("write .git file");
        assert_eq!(main_worktree(tmp.path()), None);
    }

    #[test]
    fn is_none_outside_a_repo() {
        let tmp = TempDir::new();
        assert_eq!(main_worktree(tmp.path()), None);
    }
}

mod linked_worktrees {
    use super::*;
    use crate::test_util::{git, git_repo};

    #[test]
    fn lists_the_checkouts_made_with_git() {
        let repo = git_repo(&[]);
        let (a, b) = (TempDir::new(), TempDir::new());
        let (a_path, b_path) = (a.path().join("a"), b.path().join("b"));
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "b", &b_path.display().to_string()]);
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "a", &a_path.display().to_string()]);

        let mut expected = vec![a_path.canonicalize().expect("a"), b_path.canonicalize().expect("b")];
        expected.sort();
        assert_eq!(linked_worktrees(repo.path()), expected);
    }

    #[test]
    fn leaves_out_checkouts_whose_folder_is_gone() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let path = tmp.path().join("gone");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "gone", &path.display().to_string()]);
        std::fs::remove_dir_all(&path).expect("remove worktree folder");

        assert_eq!(linked_worktrees(repo.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn is_empty_without_worktrees() {
        let repo = git_repo(&[]);
        assert_eq!(linked_worktrees(repo.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn is_an_error_when_the_registry_cannot_be_read() {
        let repo = git_repo(&[]);
        std::fs::write(repo.path().join(".git/worktrees"), "").expect("a file where the registry goes");

        assert!(registered_worktrees(repo.path()).is_err());
        assert_eq!(linked_worktrees(repo.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn is_empty_and_readable_without_worktrees() {
        let repo = git_repo(&[]);
        assert_eq!(registered_worktrees(repo.path()).expect("no registry is an empty one"), Vec::<PathBuf>::new());
    }
}

mod branch {
    use super::*;

    #[rstest]
    #[case::simple("ref: refs/heads/main\n", "main")]
    #[case::with_slashes("ref: refs/heads/feature/login\n", "feature/login")]
    #[case::detached_shows_short_sha("3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n", "3f2a9c1")]
    fn reads_head(#[case] head: &str, #[case] expected: &str) {
        let repo = repo_with_head(head);
        assert_eq!(branch(repo.path()).as_deref(), Some(expected));
    }

    #[test]
    fn is_found_from_a_subfolder() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let sub = repo.path().join("src/deep");
        std::fs::create_dir_all(&sub).expect("create subfolder");
        assert_eq!(branch(&sub).as_deref(), Some("main"));
    }

    #[test]
    fn follows_a_gitdir_file() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let worktrees = repo.path().join(".git/worktrees/wt");
        std::fs::create_dir_all(&worktrees).expect("create worktree gitdir");
        std::fs::write(worktrees.join("HEAD"), "ref: refs/heads/hotfix\n").expect("write HEAD");
        let wt = repo.path().join("wt");
        std::fs::create_dir(&wt).expect("create worktree");
        std::fs::write(wt.join(".git"), "gitdir: ../.git/worktrees/wt\n").expect("write .git file");
        assert_eq!(branch(&wt).as_deref(), Some("hotfix"));
    }

    #[test]
    fn is_none_outside_a_repo() {
        let tmp = TempDir::new();
        assert_eq!(branch(tmp.path()), None);
    }
}

mod checked_out {
    use super::*;

    #[rstest]
    #[case::a_branch("ref: refs/heads/feat/login\n", Some("feat/login"))]
    #[case::a_detached_head("3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n", None)]
    fn is_the_branch_only(#[case] head: &str, #[case] expected: Option<&str>) {
        let repo = repo_with_head(head);
        assert_eq!(checked_out(repo.path()).as_deref(), expected);
    }
}

struct Cloned {
    _dirs: [TempDir; 3],
    clone: PathBuf,
    worktree: PathBuf,
}

fn cloned_with_a_worktree(default_branch: &str) -> Cloned {
    use crate::test_util::{clone_of, git, git_repo};
    let remote = git_repo(&[]);
    git(remote.path(), &["branch", "-m", default_branch]);
    let (dir, clone) = clone_of(remote.path());
    let tmp = TempDir::new();
    let worktree = tmp.path().join("wt");
    git(&clone, &["worktree", "add", "--quiet", "-b", "feat", &worktree.display().to_string()]);
    Cloned { _dirs: [remote, dir, tmp], clone, worktree }
}

mod origin_head {
    use super::*;
    use crate::test_util::git_repo;

    #[test]
    fn is_the_default_branch_of_the_clone_also_from_a_worktree() {
        let cloned = cloned_with_a_worktree("trunk");

        assert_eq!(origin_head(&cloned.clone).as_deref(), Some("trunk"));
        assert_eq!(origin_head(&cloned.worktree).as_deref(), Some("trunk"));
    }

    #[test]
    fn is_none_without_a_clone() {
        assert_eq!(origin_head(git_repo(&[]).path()), None);
    }
}

mod has_remote {
    use super::*;
    use crate::test_util::git_repo;

    #[test]
    fn is_true_in_a_clone_and_its_worktrees() {
        let cloned = cloned_with_a_worktree("main");

        assert!(has_remote(&cloned.clone));
        assert!(has_remote(&cloned.worktree));
    }

    #[test]
    fn is_false_for_a_repo_of_its_own_or_no_repo() {
        assert!(!has_remote(git_repo(&[]).path()));
        assert!(!has_remote(TempDir::new().path()));
    }
}

mod remote_urls {
    use super::*;
    use crate::test_util::git_repo;

    #[test]
    fn lists_each_remote_url_from_the_shared_config_also_in_a_worktree() {
        let cloned = cloned_with_a_worktree("main");
        let urls = remote_urls(&cloned.worktree);

        assert_eq!(urls.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["origin"]);
        assert_eq!(remote_urls(&cloned.clone), urls);
    }

    #[test]
    fn reads_only_url_lines_of_remote_sections() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        std::fs::write(
            repo.path().join(".git/config"),
            "[core]\n\turl = nope\n[remote \"origin\"]\n\turl = git@gitlab.com:acme/shop.git\n\turlx = no\n\
             [remote \"up\"]\n\turl=https://github.com/acme/shop\n[branch \"main\"]\n\tremote = origin\n",
        )
        .expect("write config");

        assert_eq!(
            remote_urls(repo.path()),
            [
                ("origin".to_string(), "git@gitlab.com:acme/shop.git".to_string()),
                ("up".to_string(), "https://github.com/acme/shop".to_string()),
            ]
        );
        assert_eq!(remote_urls(git_repo(&[]).path()), []);
    }
}

mod primary_remote {
    use crate::git::remote;
    use crate::test_util::git_repo;

    fn named(config: &str) -> Option<String> {
        let repo = git_repo(&[]);
        std::fs::write(repo.path().join(".git/config"), config).expect("write config");
        remote::primary(repo.path()).map(|r| r.name)
    }

    #[test]
    fn is_origin_wherever_it_is_listed() {
        let config =
            "[remote \"up\"]\n\turl = git@gitlab.com:a/b.git\n[remote \"origin\"]\n\turl = git@github.com:a/b.git\n";

        assert_eq!(named(config).as_deref(), Some("origin"));
    }

    #[test]
    fn is_the_first_remote_without_an_origin() {
        let config =
            "[remote \"up\"]\n\turl = git@gitlab.com:a/b.git\n[remote \"mirror\"]\n\turl = git@github.com:a/b.git\n";

        assert_eq!(named(config).as_deref(), Some("up"));
    }

    #[test]
    fn is_none_without_a_remote_or_with_a_local_path() {
        assert_eq!(named("[core]\n"), None);
        assert_eq!(named("[remote \"origin\"]\n\turl = /srv/git/shop\n"), None);
    }
}

mod repo_identity {
    use std::cell::RefCell;

    use super::*;
    use crate::git::remote::identity;

    const SHOP: &str = "github.com/acme/web-shop";

    fn listed(remotes: &[(&str, &str)]) -> Vec<(String, String)> {
        remotes.iter().map(|(name, url)| ((*name).to_string(), (*url).to_string())).collect()
    }

    fn aliases(host: &str) -> Option<String> {
        match host {
            "gh-work" => Some("GitHub.com".into()),
            "github.com" => Some("github.com".into()),
            _ => None,
        }
    }

    #[rstest]
    #[case::scp_like(&[("origin", "git@github.com:acme/web-shop.git")], Some(SHOP))]
    #[case::https(&[("origin", "https://github.com/acme/web-shop")], Some(SHOP))]
    #[case::https_with_dot_git(&[("origin", "https://github.com/acme/web-shop.git")], Some(SHOP))]
    #[case::ssh_scheme(&[("origin", "ssh://git@github.com/acme/web-shop")], Some(SHOP))]
    #[case::ssh_scheme_with_a_port(&[("origin", "ssh://git@github.com:2222/acme/web-shop/")], Some(SHOP))]
    #[case::an_ssh_alias(&[("origin", "gh-work:acme/web-shop.git")], Some(SHOP))]
    #[case::upper_case(&[("origin", "https://GitHub.com/Acme/Web-Shop.git")], Some(SHOP))]
    #[case::a_fork(&[("origin", "git@github.com:me/web-shop.git"), ("upstream", "https://github.com/acme/web-shop")], Some(SHOP))]
    #[case::origin_before_others(&[("mirror", "https://gitlab.com/x/y"), ("origin", "git@github.com:acme/web-shop")], Some(SHOP))]
    #[case::the_first_without_upstream_or_origin(&[("mine", "https://github.com/acme/web-shop"), ("other", "https://gitlab.com/x/y")], Some(SHOP))]
    #[case::the_first_that_parses(&[("upstream", "/srv/git/shop"), ("origin", "https://github.com/acme/web-shop")], Some(SHOP))]
    #[case::the_first_with_a_path(&[("upstream", "https://github.com"), ("origin", "https://github.com/acme/web-shop")], Some(SHOP))]
    #[case::an_upper_case_suffix(&[("origin", "https://github.com/acme/web-shop.GIT")], Some(SHOP))]
    #[case::an_unresolved_alias(&[("origin", "box:acme/web-shop")], Some("box/acme/web-shop"))]
    #[case::no_remotes(&[], None)]
    #[case::only_a_local_path(&[("origin", "/srv/git/shop")], None)]
    #[case::no_path(&[("origin", "https://github.com")], None)]
    fn is_host_and_path_in_lower_case(#[case] remotes: &[(&str, &str)], #[case] expected: Option<&str>) {
        assert_eq!(identity(&listed(remotes), aliases).as_deref(), expected);
    }

    #[test]
    fn resolves_only_the_host_of_ssh_urls() {
        let asked = RefCell::new(Vec::new());
        let resolve = |host: &str| {
            asked.borrow_mut().push(host.to_string());
            Some("elsewhere.example".to_string())
        };

        assert_eq!(
            identity(&listed(&[("origin", "https://github.com/acme/web-shop")]), resolve).as_deref(),
            Some(SHOP)
        );
        assert_eq!(identity(&listed(&[("origin", "http://github.com/acme/web-shop")]), resolve).as_deref(), Some(SHOP));
        assert!(asked.borrow().is_empty());

        for url in [
            "ssh://github.com/acme/web-shop",
            "git+ssh://github.com/acme/web-shop",
            "ssh+git://github.com/acme/web-shop",
        ] {
            let ssh = identity(&listed(&[("origin", url)]), resolve);

            assert_eq!(ssh.as_deref(), Some("elsewhere.example/acme/web-shop"));
        }
        assert_eq!(*asked.borrow(), ["github.com", "github.com", "github.com"]);
    }
}

mod remote_without_credentials {
    use super::*;
    use crate::git::remote::without_credentials;

    #[rstest]
    #[case::a_user_and_password(
        "https://ana:s3cret@github.com/acme/web-shop.git",
        "https://github.com/acme/web-shop.git"
    )]
    #[case::a_token("https://ghp_abc123@github.com/acme/web-shop", "https://github.com/acme/web-shop")]
    #[case::an_at_in_the_password("https://ana:p@ss@github.com:8443/acme/shop", "https://github.com:8443/acme/shop")]
    #[case::an_ssh_user("ssh://git@github.com:2222/acme/web-shop", "ssh://git@github.com:2222/acme/web-shop")]
    #[case::an_ssh_password("ssh://git:s3cret@github.com/acme/web-shop", "ssh://git@github.com/acme/web-shop")]
    #[case::only_an_ssh_password("ssh://:s3cret@github.com/acme/web-shop", "ssh://github.com/acme/web-shop")]
    #[case::an_upper_case_scheme("HTTPS://ghp_abc123@github.com/acme/web-shop", "HTTPS://github.com/acme/web-shop")]
    #[case::no_path("https://token@github.com", "https://github.com")]
    #[case::an_at_in_the_path("https://github.com/acme/web@shop", "https://github.com/acme/web@shop")]
    #[case::no_credentials("https://github.com/acme/web-shop", "https://github.com/acme/web-shop")]
    #[case::scp_like("git@github.com:acme/web-shop.git", "git@github.com:acme/web-shop.git")]
    #[case::scp_like_with_a_query_and_a_fragment(
        "git@github.com:acme/shop?v2#x.git",
        "git@github.com:acme/shop?v2#x.git"
    )]
    #[case::a_local_path("/srv/git/shop", "/srv/git/shop")]
    #[case::a_stray_quote("\"https://ghp_abc123@github.com/acme/shop\"", "\"https://github.com/acme/shop\"")]
    #[case::a_git_scheme_user("git://ghp_abc123@github.com/acme/shop", "git://github.com/acme/shop")]
    #[case::a_git_ssh_user("GIT+SSH://git:s3cret@github.com/acme/shop", "GIT+SSH://git@github.com/acme/shop")]
    #[case::a_query("https://github.com/acme/shop.git?access_token=ghp_abc123", "https://github.com/acme/shop.git")]
    #[case::a_fragment("https://github.com/acme/shop#ghp_abc123", "https://github.com/acme/shop")]
    #[case::a_query_with_no_path("https://github.com?access_token=ghp_abc123", "https://github.com")]
    #[case::a_user_and_a_query("https://ana:s3cret@github.com/acme/shop?x=1#y", "https://github.com/acme/shop")]
    #[case::a_question_mark_in_the_password("https://ana:p?ss@github.com/acme/shop", "https://github.com/acme/shop")]
    #[case::an_ssh_user_and_a_query("ssh://git@github.com/acme/shop?key=s3cret", "ssh://git@github.com/acme/shop")]
    fn keeps_everything_but_the_secrets(#[case] url: &str, #[case] expected: &str) {
        assert_eq!(without_credentials(url), expected);
    }
}

mod user_name {
    use super::*;

    #[rstest]
    #[case::plain("[user]\n\tname = Ada Lovelace\n", Some("Ada Lovelace"))]
    #[case::quoted("[core]\n\tname = x\n[User]\n  Name = \"Ada\"\n", Some("Ada"))]
    #[case::last_wins("[user]\nname = a\n[user]\nname = b\n", Some("b"))]
    #[case::other_section("[remote \"origin\"]\n\tname = x\n", None)]
    #[case::empty("[user]\n\tname =\n", None)]
    fn reads_the_user_section(#[case] config: &str, #[case] expected: Option<&str>) {
        assert_eq!(config_user(config).as_deref(), expected);
    }

    #[test]
    fn the_first_file_that_names_one_wins() {
        let dir = TempDir::new();
        let (repo, global) = (dir.path().join("repo"), dir.path().join("global"));
        std::fs::write(&repo, "[core]\n").expect("write repo config");
        std::fs::write(&global, "[user]\nname = Grace\n").expect("write global config");
        let missing = dir.path().join("missing");
        assert_eq!(user_name_in(&[missing, repo, global]).as_deref(), Some("Grace"));
    }
}

mod index_and_objects {
    use super::*;
    use crate::test_util::{git, git_repo};

    #[test]
    fn a_linked_worktree_has_its_own_index_and_shares_the_objects() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let path = tmp.path().join("wt");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "wt", &path.display().to_string()]);

        let index = index_file(&path).expect("index of the worktree");
        assert_eq!(index.parent().and_then(Path::parent).and_then(Path::file_name), Some("worktrees".as_ref()));
        assert_eq!(index.file_name(), Some("index".as_ref()));
        let objects = objects_dir(&path).and_then(|dir| dir.canonicalize().ok());
        assert_eq!(objects, Some(repo.path().join(".git/objects")));
    }

    #[test]
    fn the_main_checkout_keeps_both_in_its_git_dir() {
        let repo = git_repo(&[]);
        assert_eq!(index_file(repo.path()), Some(repo.path().join(".git/index")));
        assert_eq!(objects_dir(repo.path()), Some(repo.path().join(".git/objects")));
    }
}
