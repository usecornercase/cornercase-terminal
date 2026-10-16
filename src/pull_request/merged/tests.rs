use rstest::rstest;

use super::*;
use crate::git::remote::remote;
use crate::test_util::{TempDir, fake_gh, fake_glab, fixture};

const NOW: u64 = 1_792_065_600;
const GH_STATUS: &str = fixture!("gh/2.101.0/auth-status.json");
const GH_SEARCH: &str = fixture!("gh/2.101.0/merged-search.json");
const GLAB_STATUS: &str = fixture!("glab/1.123.0/auth-status.json");
const GLAB_MERGES: &str = fixture!("glab/1.123.0/merged-requests.json");

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(NOW)
}

fn since() -> SystemTime {
    now() - Duration::from_hours(40 * 24)
}

fn remotes(urls: &[&str]) -> Vec<Remote> {
    urls.iter().filter_map(|url| remote("origin", url)).collect()
}

fn source(provider: Provider, host: &str, login: &str) -> Source {
    Source { provider, host: host.into(), login: login.into(), ssh_host: None, subfolder: String::new() }
}

fn merged(host: &str, path: &str, number: u64, ago_days: u64) -> Merged {
    Merged {
        key: Key { host: host.into(), path: path.into(), number },
        at: now() - Duration::from_hours(ago_days * 24),
    }
}

fn listed(merged: Vec<Merged>) -> Listed {
    Listed { merged, truncated: false }
}

fn account(source: Source, listed: Result<Listed, String>) -> Account {
    Account { source, listed }
}

fn answer(github: Read) -> Answer {
    Answer { github, gitlab: Read::Unavailable("none".into()) }
}

fn cache_of(accounts: Vec<Account>) -> Cache {
    let mut cache = Cache::default();
    cache.apply(answer(Read::Accounts(accounts)));
    cache
}

fn counted(cache: &Cache, urls: &[&str]) -> Option<[u64; 3]> {
    cache.count(&remotes(urls), since(), now()).map(|row| [row.day, row.week, row.total])
}

const SHOP: &str = "git@github.com:acme/web-shop.git";

mod github_discovery {
    use super::*;

    fn gh(tmp: &TempDir, status: &str) -> std::path::PathBuf {
        std::fs::write(tmp.path().join("status.json"), status).expect("status");
        std::fs::write(tmp.path().join("search.json"), GH_SEARCH).expect("search");
        fake_gh(
            tmp.path(),
            &format!(
                "D={dir}\necho \"$* token=${{GH_TOKEN:+yes}}\" >> $D/log\n\
                 case \"$1 $2\" in\n\
                 'auth status') cat $D/status.json;;\n\
                 'auth token') echo \"token-$6\";;\n\
                 'api graphql') cat $D/search.json;;\n\
                 esac",
                dir = crate::agents::quote(&tmp.path().display().to_string())
            ),
        )
    }

    fn accounts(read: Read) -> Vec<Account> {
        match read {
            Read::Accounts(accounts) => accounts,
            other => panic!("expected accounts, got {other:?}"),
        }
    }

    #[test]
    fn keeps_the_accounts_that_work_and_skips_the_broken_one() {
        let tmp = TempDir::new();

        let found = accounts(github::read(&gh(&tmp, GH_STATUS), tmp.path(), since(), now()));

        let names: Vec<_> = found.iter().map(|a| (a.source.host.as_str(), a.source.login.as_str())).collect();
        assert_eq!(names, [("ghe.acme.dev", "ana"), ("github.com", "ana"), ("github.com", "ana-work")]);
    }

    #[test]
    fn reads_the_token_of_an_inactive_account_and_never_switches() {
        let tmp = TempDir::new();

        accounts(github::read(&gh(&tmp, GH_STATUS), tmp.path(), since(), now()));

        let log = std::fs::read_to_string(tmp.path().join("log")).expect("log");
        let calls: Vec<&str> = log.lines().collect();
        assert!(calls.iter().any(|c| c.starts_with("auth token --hostname github.com --user ana-work")));
        assert!(calls.iter().all(|c| !c.contains("switch") && !c.contains("login") && !c.contains("logout")));
        let queries: Vec<&&str> = calls.iter().filter(|c| c.starts_with("api graphql")).collect();
        assert_eq!(queries.len(), 3);
        assert_eq!(queries.iter().filter(|c| c.ends_with("token=yes")).count(), 1);
        assert!(queries.iter().all(|c| c.contains("--hostname")));
    }

    #[test]
    fn counts_every_page_and_names_repositories_in_lower_case() {
        let tmp = TempDir::new();

        let found = accounts(github::read(&gh(&tmp, GH_STATUS), tmp.path(), since(), now()));

        let one = found[1].listed.as_ref().expect("listed");
        let numbers: Vec<_> = one.merged.iter().map(|m| (m.key.path.as_str(), m.key.number)).collect();
        assert_eq!(numbers, [("acme/web-shop", 1), ("acme/web-shop", 2), ("acme/web-shop", 3), ("elsewhere/other", 9)]);
        assert!(!one.truncated);
    }

    #[test]
    fn leaves_out_what_merged_before_counting_began() {
        let tmp = TempDir::new();
        let late = now() - Duration::from_hours(3 * 24);

        let found = accounts(github::read(&gh(&tmp, GH_STATUS), tmp.path(), late, now()));

        let one = found[1].listed.as_ref().expect("listed");
        assert_eq!(one.merged.iter().map(|m| m.key.number).collect::<Vec<_>>(), [1, 9]);
    }

    #[rstest]
    #[case::none("{\"hosts\":{}}")]
    #[case::only_broken(r#"{"hosts":{"github.com":[{"state":"error","active":false,"login":"x"}]}}"#)]
    fn nobody_logged_in_is_unavailable(#[case] status: &str) {
        let tmp = TempDir::new();

        let read = github::read(&gh(&tmp, status), tmp.path(), since(), now());

        assert!(matches!(read, Read::Unavailable(_)), "{read:?}");
    }

    #[test]
    fn a_missing_gh_is_unavailable() {
        let tmp = TempDir::new();

        let read = github::read(&tmp.path().join("no-gh"), tmp.path(), since(), now());

        assert!(matches!(read, Read::Unavailable(why) if why.contains("install gh")));
    }

    #[test]
    fn a_gh_without_json_status_is_unavailable() {
        let tmp = TempDir::new();
        let old = fake_gh(tmp.path(), "echo 'unknown flag: --json' >&2; exit 1");

        let read = github::read(&old, tmp.path(), since(), now());

        assert!(matches!(read, Read::Unavailable(why) if why.contains("update gh")));
    }

    #[test]
    fn a_failing_query_is_an_error_of_that_account_only() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join("status.json"), GH_STATUS).expect("status");
        let gh = fake_gh(
            tmp.path(),
            &format!(
                "case \"$1 $2\" in\n'auth status') cat {dir}/status.json;;\n'auth token') echo secret-$6;;\n\
                 'api graphql') if [ -n \"$GH_TOKEN\" ]; then echo \"bad $GH_TOKEN\" >&2; exit 1; fi; echo '{{\"data\":{{\"search\":{{\"issueCount\":0,\"nodes\":[]}}}}}}';;\nesac",
                dir = crate::agents::quote(&tmp.path().display().to_string())
            ),
        );

        let found = accounts(github::read(&gh, tmp.path(), since(), now()));

        let errors: Vec<_> = found.iter().map(|a| a.listed.as_ref().err().cloned()).collect();
        assert_eq!(errors, [None, None, Some("bad …".into())]);
    }

    #[test]
    fn splits_the_dates_when_a_search_passes_the_cap() {
        let tmp = TempDir::new();
        let recent = now() - Duration::from_hours(2 * 24);
        let gh = fake_gh(
            tmp.path(),
            &format!(
                "case \"$1 $2\" in\n'auth status') echo '{status}';;\n\
                 'api graphql') case \"$*\" in\n\
                 *'2026-10-13..2026-10-15'*) echo '{{\"data\":{{\"search\":{{\"issueCount\":1500,\"nodes\":[]}}}}}}';;\n\
                 *'2026-10-13..2026-10-14'*) echo '{{\"data\":{{\"search\":{{\"issueCount\":1,\"nodes\":[{{\"number\":1,\"mergedAt\":\"2026-10-14T10:00:00Z\",\"repository\":{{\"nameWithOwner\":\"a/b\"}}}}]}}}}}}';;\n\
                 *) echo '{{\"data\":{{\"search\":{{\"issueCount\":1,\"nodes\":[{{\"number\":2,\"mergedAt\":\"2026-10-15T10:00:00Z\",\"repository\":{{\"nameWithOwner\":\"a/b\"}}}}]}}}}}}';;\n\
                 esac;;\nesac",
                status = r#"{"hosts":{"github.com":[{"state":"success","active":true,"login":"ana"}]}}"#
            ),
        );

        let found = accounts(github::read(&gh, tmp.path(), recent, now()));

        let one = found[0].listed.as_ref().expect("listed");
        assert_eq!(one.merged.len(), 2, "{one:?}");
        assert!(!one.truncated);
    }

    #[test]
    fn a_range_that_cannot_be_split_further_is_truncated() {
        let tmp = TempDir::new();
        let gh = fake_gh(
            tmp.path(),
            &format!(
                "case \"$1 $2\" in\n'auth status') echo '{status}';;\n\
                 'api graphql') echo '{{\"data\":{{\"search\":{{\"issueCount\":5000,\"nodes\":[]}}}}}}';;\nesac",
                status = r#"{"hosts":{"github.com":[{"state":"success","active":true,"login":"ana"}]}}"#
            ),
        );

        let found = accounts(github::read(&gh, tmp.path(), now() - Duration::from_hours(2 * 24), now()));

        assert!(found[0].listed.as_ref().expect("listed").truncated);
    }
}

mod gitlab_discovery {
    use super::*;

    fn glab(tmp: &TempDir) -> std::path::PathBuf {
        std::fs::write(tmp.path().join("status.json"), GLAB_STATUS).expect("status");
        std::fs::write(tmp.path().join("page-1.json"), GLAB_MERGES).expect("merges");
        fake_glab(
            tmp.path(),
            &format!(
                "D={dir}\necho \"$*\" >> $D/log\ncase \"$1\" in\nauth) cat $D/status.json; exit 1;;\napi) for last; do :; done; p=${{last##*page=}}; cat $D/page-$p.json || exit 1;;\nesac",
                dir = crate::agents::quote(&tmp.path().display().to_string())
            ),
        )
    }

    #[test]
    fn reads_the_hosts_that_work_even_when_glab_exits_with_one() {
        let tmp = TempDir::new();

        let read = gitlab::read(&glab(&tmp), tmp.path(), since());

        let Read::Accounts(found) = read else { panic!("expected accounts") };
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source.login, "ana");
        assert_eq!(found[0].source.ssh_host.as_deref(), Some("altssh.gitlab.com"));
        let log = std::fs::read_to_string(tmp.path().join("log")).expect("log");
        assert!(log.contains("api --hostname gitlab.com merge_requests?scope=created_by_me&state=merged"));
    }

    #[test]
    fn counts_every_page_skips_unmerged_ones_and_reads_offsets() {
        let tmp = TempDir::new();

        let Read::Accounts(found) = gitlab::read(&glab(&tmp), tmp.path(), since()) else { panic!("expected accounts") };

        let one = found[0].listed.as_ref().expect("listed");
        let keys: Vec<_> = one.merged.iter().map(|m| (m.key.path.as_str(), m.key.number)).collect();
        assert_eq!(keys, [("acme/team/shop", 4), ("acme/team/shop", 5)]);
        assert_eq!(one.merged[1].at, UNIX_EPOCH + Duration::from_hours(497_671));
    }

    fn pages(tmp: &TempDir, full: u32, last: Option<usize>) {
        let page = |n: u32, count: usize| {
            let nodes: Vec<String> = (0..count)
                .map(|i| {
                    format!(
                        r#"{{"iid":{},"merged_at":"2026-10-15T09:00:00.000Z","references":{{"full":"acme/shop!{}"}}}}"#,
                        n as usize * 1000 + i,
                        n as usize * 1000 + i
                    )
                })
                .collect();
            std::fs::write(tmp.path().join(format!("page-{n}.json")), format!("[{}]", nodes.join(","))).expect("page");
        };
        for n in 1..=full {
            page(n, 100);
        }
        if let Some(count) = last {
            page(full + 1, count);
        }
    }

    fn first(read: Read) -> Listed {
        let Read::Accounts(found) = read else { panic!("expected accounts") };
        found[0].listed.clone().expect("listed")
    }

    #[test]
    fn a_short_page_ends_the_list_complete() {
        let tmp = TempDir::new();
        let glab = glab(&tmp);
        pages(&tmp, 1, Some(3));

        let listed = first(gitlab::read(&glab, tmp.path(), since()));

        assert_eq!((listed.merged.len(), listed.truncated), (103, false));
        let log = std::fs::read_to_string(tmp.path().join("log")).expect("log");
        assert_eq!(log.lines().filter(|l| l.starts_with("api")).count(), 2);
    }

    #[test]
    fn stops_at_the_page_bound_and_says_the_count_is_partial() {
        let tmp = TempDir::new();
        let glab = glab(&tmp);
        pages(&tmp, gitlab::MAX_PAGES, Some(100));

        let listed = first(gitlab::read(&glab, tmp.path(), since()));

        assert_eq!((listed.merged.len(), listed.truncated), (100 * gitlab::MAX_PAGES as usize, true));
        let log = std::fs::read_to_string(tmp.path().join("log")).expect("log");
        assert_eq!(log.lines().filter(|l| l.starts_with("api")).count(), gitlab::MAX_PAGES as usize);
    }

    #[test]
    fn a_later_page_failing_keeps_the_pages_read_as_partial() {
        let tmp = TempDir::new();
        let glab = glab(&tmp);
        pages(&tmp, 1, None);

        let listed = first(gitlab::read(&glab, tmp.path(), since()));

        assert_eq!((listed.merged.len(), listed.truncated), (100, true));
    }

    #[test]
    fn the_first_page_failing_is_an_error_of_the_account() {
        let tmp = TempDir::new();
        let glab = glab(&tmp);
        let _ = std::fs::remove_file(tmp.path().join("page-1.json"));

        let Read::Accounts(found) = gitlab::read(&glab, tmp.path(), since()) else { panic!("expected accounts") };

        assert!(found[0].listed.is_err());
    }

    #[rstest]
    #[case::missing_flag("echo 'unknown flag: --output' >&2; exit 1", "update glab")]
    #[case::none_logged_in("echo 'no GitLab instances have been authenticated' >&2; exit 1", "not logged in")]
    #[case::none_succeeded(r#"echo '{"hosts":[{"host":"x","state":"error"}]}'; exit 1"#, "not logged in")]
    fn unavailable_when_nothing_can_be_read(#[case] script: &str, #[case] reason: &str) {
        let tmp = TempDir::new();

        let read = gitlab::read(&fake_glab(tmp.path(), script), tmp.path(), since());

        assert!(matches!(read, Read::Unavailable(ref why) if why.contains(reason)), "{read:?}");
    }

    #[test]
    fn a_missing_glab_is_unavailable() {
        let tmp = TempDir::new();

        let read = gitlab::read(&tmp.path().join("no-glab"), tmp.path(), since());

        assert!(matches!(read, Read::Unavailable(why) if why.contains("install glab")));
    }

    #[test]
    fn another_failure_keeps_the_sources() {
        let tmp = TempDir::new();

        let read =
            gitlab::read(&fake_glab(tmp.path(), "echo 'dial tcp: i/o timeout' >&2; exit 1"), tmp.path(), since());

        assert!(matches!(read, Read::Failed(why) if why.contains("timeout")));
    }
}

mod counting {
    use super::*;

    fn github() -> Source {
        source(Provider::Github, "github.com", "ana")
    }

    #[test]
    fn a_successful_empty_answer_is_a_real_zero() {
        let cache = cache_of(vec![account(github(), Ok(listed(Vec::new())))]);

        assert_eq!(counted(&cache, &[SHOP]), Some([0, 0, 0]));
        assert!(cache.count(&remotes(&[SHOP]), since(), now()).is_some_and(|row| row.complete));
    }

    #[rstest]
    #[case::nothing_asked(Cache::default())]
    #[case::first_query_failed(cache_of(vec![account(source(Provider::Github, "github.com", "ana"), Err("boom".into()))]))]
    fn hides_the_row_until_something_succeeded(#[case] cache: Cache) {
        assert_eq!(counted(&cache, &[SHOP]), None);
    }

    #[test]
    fn counts_the_windows_from_the_merge_time() {
        let cache = cache_of(vec![account(
            github(),
            Ok(listed(vec![
                merged("github.com", "acme/web-shop", 1, 0),
                merged("github.com", "acme/web-shop", 2, 3),
                merged("github.com", "acme/web-shop", 3, 20),
                merged("github.com", "acme/web-shop", 4, 60),
            ])),
        )]);

        assert_eq!(counted(&cache, &[SHOP]), Some([1, 2, 3]));
    }

    #[test]
    fn leaves_out_repositories_of_no_project() {
        let cache = cache_of(vec![account(
            github(),
            Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0), merged("github.com", "other/repo", 2, 0)])),
        )]);

        assert_eq!(counted(&cache, &[SHOP]), Some([1, 1, 1]));
        assert_eq!(counted(&cache, &[]), None);
    }

    #[test]
    fn counts_a_pull_request_once_whatever_lists_it() {
        let same = vec![merged("github.com", "acme/web-shop", 1, 0)];
        let cache = cache_of(vec![
            account(github(), Ok(listed(same.clone()))),
            account(source(Provider::Github, "github.com", "ANA"), Ok(listed(same.clone()))),
            account(source(Provider::Github, "github.com", "ana-work"), Ok(listed(same))),
        ]);

        let both = ["git@github.com:acme/web-shop.git", "https://github.com/acme/web-shop"];
        assert_eq!(counted(&cache, &both), Some([1, 1, 1]));
        assert_eq!(cache.accounts().len(), 2);
    }

    #[test]
    fn no_open_project_with_a_remote_hides_the_row() {
        let cache = cache_of(vec![account(github(), Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])))]);

        assert_eq!(counted(&cache, &[]), None);
    }

    #[rstest]
    #[case::bitbucket("git@bitbucket.org:acme/web-shop.git")]
    #[case::gitea("https://git.acme.dev/acme/web-shop.git")]
    fn a_remote_on_another_service_never_matches_by_path(#[case] url: &str) {
        let cache = cache_of(vec![account(github(), Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])))]);

        assert_eq!(counted(&cache, &[url]), Some([0, 0, 0]));
    }

    #[test]
    fn an_alias_without_a_dot_matches_by_path() {
        let cache = cache_of(vec![account(github(), Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])))]);

        assert_eq!(counted(&cache, &["git@work:acme/web-shop.git"]), Some([1, 1, 1]));
    }

    #[rstest]
    #[case::scp("git@github.com:acme/web-shop.git")]
    #[case::https("https://github.com/acme/web-shop.git")]
    #[case::alternate_ssh("ssh://git@ssh.github.com:443/acme/web-shop.git")]
    #[case::an_alias_matched_by_path("git@github.com-work:acme/web-shop.git")]
    fn a_remote_places_its_repository(#[case] url: &str) {
        let cache = cache_of(vec![account(github(), Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])))]);

        assert_eq!(counted(&cache, &[url]), Some([1, 1, 1]));
    }

    #[test]
    fn a_placed_remote_does_not_match_the_same_path_on_another_host() {
        let cache = cache_of(vec![
            account(github(), Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)]))),
            account(
                source(Provider::Github, "ghe.acme.dev", "ana"),
                Ok(listed(vec![merged("ghe.acme.dev", "acme/web-shop", 2, 0)])),
            ),
        ]);

        assert_eq!(counted(&cache, &["git@ghe.acme.dev:acme/web-shop.git"]), Some([1, 1, 1]));
    }

    #[test]
    fn a_gitlab_remote_matches_by_ssh_host_and_subfolder() {
        let mut host = source(Provider::Gitlab, "acme.dev", "ana");
        host.ssh_host = Some("git.acme.dev".into());
        host.subfolder = "gitlab".into();
        let mut cache = Cache::default();
        cache.apply(Answer {
            github: Read::Unavailable("none".into()),
            gitlab: Read::Accounts(vec![account(host, Ok(listed(vec![merged("acme.dev", "team/shop", 4, 0)])))]),
        });

        assert_eq!(counted(&cache, &["https://acme.dev/gitlab/team/shop.git"]), Some([1, 1, 1]));
        assert_eq!(counted(&cache, &["git@git.acme.dev:team/shop.git"]), Some([1, 1, 1]));
    }
}

mod partial {
    use super::*;

    fn two(first: Result<Listed, String>, second: Result<Listed, String>) -> Cache {
        cache_of(vec![
            account(source(Provider::Github, "github.com", "ana"), first),
            account(source(Provider::Github, "github.com", "ana-work"), second),
        ])
    }

    fn row(cache: &Cache) -> MergedRow {
        cache.count(&remotes(&[SHOP]), since(), now()).expect("a row")
    }

    #[test]
    fn one_account_failing_makes_the_count_partial() {
        let cache = two(Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])), Err("boom".into()));

        let row = row(&cache);

        assert_eq!((row.total, row.complete), (1, false));
        assert_eq!(row.accounts[1].error.as_deref(), Some("boom"));
    }

    #[test]
    fn a_later_failure_keeps_the_last_list() {
        let mut cache = two(Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])), Ok(listed(Vec::new())));
        cache.apply(answer(Read::Accounts(vec![
            account(source(Provider::Github, "github.com", "ana"), Err("boom".into())),
            account(source(Provider::Github, "github.com", "ana-work"), Ok(listed(Vec::new()))),
        ])));

        let row = row(&cache);

        assert_eq!((row.total, row.complete), (1, false));
    }

    #[test]
    fn a_failed_discovery_keeps_the_lists_and_makes_the_count_partial() {
        let mut cache = two(Ok(listed(vec![merged("github.com", "acme/web-shop", 1, 0)])), Ok(listed(Vec::new())));
        cache.apply(answer(Read::Failed("offline".into())));

        let row = row(&cache);

        assert_eq!((row.total, row.complete), (1, false));
        assert_eq!(cache.problems().len(), 3);
        assert_eq!(cache.note(), "offline");
    }

    #[test]
    fn a_truncated_list_makes_the_count_partial() {
        let cache = two(Ok(Listed { merged: Vec::new(), truncated: true }), Ok(listed(Vec::new())));

        assert!(!row(&cache).complete);
    }

    #[test]
    fn an_unavailable_provider_is_not_partial() {
        let mut cache = two(Ok(listed(Vec::new())), Ok(listed(Vec::new())));
        cache.apply(Answer {
            github: Read::Accounts(vec![account(
                source(Provider::Github, "github.com", "ana"),
                Ok(listed(Vec::new())),
            )]),
            gitlab: Read::Unavailable("none".into()),
        });

        assert!(row(&cache).complete);
        assert_eq!(cache.accounts().len(), 1);
    }

    #[test]
    fn a_recovered_account_clears_its_error() {
        let mut cache = two(Err("boom".into()), Ok(listed(Vec::new())));
        cache.apply(answer(Read::Accounts(vec![
            account(source(Provider::Github, "github.com", "ana"), Ok(listed(Vec::new()))),
            account(source(Provider::Github, "github.com", "ana-work"), Ok(listed(Vec::new()))),
        ])));

        assert!(row(&cache).complete);
        assert_eq!(cache.problems().len(), 0);
    }
}
