use std::path::{Path, PathBuf};

use super::*;
use crate::plugins::registry::{Entry, Source};

fn evil() -> Plugin {
    let entry = Entry {
        id: "notes\u{202e}".into(),
        source: Source::Link { path: PathBuf::from("/plugins/evil\u{202e}lmth.sh\u{e0041}") },
        enabled: false,
    };
    Plugin { entry, manifest: Err("cannot read it".into()) }
}

#[test]
fn the_list_shows_invisible_characters_escaped() {
    let text = list_text(&[evil()]);
    assert!(text.contains("notes\\u{202e}  -  off") && text.contains("evil\\u{202e}lmth.sh\\u{e0041}"), "{text}");
}

#[test]
fn the_json_list_escapes_invisible_characters_and_keeps_their_values() {
    let json = list_json(&[evil()], &Places::new(Path::new("/config"), Path::new("/state")));
    assert!(!json.chars().any(|c| c != '\n' && crate::text::hidden(c)), "{json}");
    assert!(json.contains("evil\\u202elmth.sh\\udb40\\udc41"), "{json}");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["plugins"][0]["id"], "notes\u{202e}");
}

fn info(actions: Vec<crate::control::plugins::ActionInfo>) -> ActionsInfo {
    ActionsInfo { revision: 2, actions }
}

fn listed(plugins: Vec<PluginInfo>) -> PluginsInfo {
    PluginsInfo { revision: Some(3), problem: None, plugins }
}

fn on_plugin() -> PluginInfo {
    PluginInfo {
        id: "acme.notes".into(),
        enabled: true,
        state: "on".into(),
        label: "Notes 1.0".into(),
        note: "service sync runs".into(),
        services: vec![
            ServiceInfo { id: "sync".into(), state: "running".into(), pid: Some(4242), ..ServiceInfo::default() },
            ServiceInfo { id: "watch".into(), state: "stopped".into(), restart_in: Some(3), ..ServiceInfo::default() },
            ServiceInfo { id: "late".into(), state: "starting".into(), ..ServiceInfo::default() },
            ServiceInfo { id: "odd".into(), state: "paused".into(), ..ServiceInfo::default() },
        ],
        button: Some(ButtonInfo { title: "Notes".into(), icon: "N".into(), actions: vec!["open".into()] }),
        ..PluginInfo::default()
    }
}

#[test]
fn the_status_names_each_plugin_its_note_and_what_each_service_is_doing() {
    let broken = PluginInfo {
        id: "acme.broken".into(),
        state: "cannot-run".into(),
        label: "acme.broken".into(),
        note: "cannot read it".into(),
        ..PluginInfo::default()
    };
    let text = status_text(&listed(vec![on_plugin(), broken]));
    assert_eq!(
        text,
        "\
revision 3
acme.notes  on  Notes 1.0
  service sync runs
  sync: running, pid 4242
  watch: stopped, restarts in 3s
  late: starting
  odd: paused
  button Notes (N): open
acme.broken  cannot-run  acme.broken
  cannot read it
"
    );
}

#[test]
fn the_status_says_when_there_are_no_plugins_and_when_the_registry_cannot_be_read() {
    assert_eq!(status_text(&PluginsInfo::default()), "no plugins\n");
    let unread = PluginsInfo { revision: Some(4), problem: Some("cannot read plugins.json".into()), plugins: vec![] };
    assert_eq!(status_text(&unread), "revision 4\ncannot read plugins.json\n");
}

#[test]
fn the_status_json_escapes_invisible_characters_and_keeps_their_values() {
    let plugin = PluginInfo {
        id: "notes\u{202e}".into(),
        source: serde_json::json!("/plugins/evil\u{202e}lmth.sh"),
        ..PluginInfo::default()
    };
    let json = shown_json(&listed(vec![plugin]));
    assert!(!json.chars().any(|c| c != '\n' && crate::text::hidden(c)), "{json}");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["plugins"][0]["id"], "notes\u{202e}");
    assert_eq!(read["plugins"][0]["source"], "/plugins/evil\u{202e}lmth.sh");
}

#[test]
fn the_status_escapes_invisible_characters() {
    let plugin = PluginInfo {
        id: "notes\u{202e}".into(),
        state: "off".into(),
        label: "Notes".into(),
        note: "a\nb".into(),
        ..PluginInfo::default()
    };
    let text = status_text(&listed(vec![plugin]));
    assert!(!text.chars().any(|c| c != '\n' && c.is_control()), "{text}");
    assert!(text.contains("notes\\u{202e}  off  Notes") && text.contains("  a b"), "{text}");
}

#[test]
fn the_actions_are_a_tab_separated_table_under_a_header() {
    let action = crate::control::plugins::ActionInfo {
        plugin: "acme.notes".into(),
        name: "Notes".into(),
        action: "note".into(),
        title: "take\ta\nnote".into(),
        contexts: vec!["pane".into(), "tab".into()],
        ..Default::default()
    };
    assert_eq!(
        actions_text(&info(vec![action])),
        "plugin\taction\tcontexts\ttitle\nacme.notes\tnote\tpane,tab\ttake a note\n"
    );
    assert_eq!(actions_text(&info(Vec::new())), "plugin\taction\tcontexts\ttitle\n");
}

#[test]
fn a_run_says_how_it_ended() {
    let ran = |error: Option<&str>| ActionRan {
        plugin: "acme.notes".into(),
        action: "note".into(),
        error: error.map(str::to_string),
        ..ActionRan::default()
    };
    assert_eq!(ran_text(&ran(None), Some(Item::Pane(5))), "ran acme.notes note on pane 5: exit status: 0\n");
    assert_eq!(
        ran_text(&ran(Some("exit status: 3")), Some(Item::Tab(2))),
        "ran acme.notes note on tab 2: exit status: 3; `cornercase plugin logs acme.notes` says more\n"
    );
    assert_eq!(
        ran_text(&ran(Some("the popup was closed")), None).lines().next(),
        Some("ran acme.notes note: the popup was closed; `cornercase plugin logs acme.notes` says more")
    );
}

mod previewing {
    use super::*;
    use crate::cli::args::{Command as Cli, PluginAction};
    use crate::cli::tests::parse;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn parses(args: &[&str]) -> bool {
        let mut line = vec!["plugin"];
        line.extend(args);
        parse(&line).is_ok()
    }

    fn install(args: &[&str]) -> InstallArgs {
        let mut line = vec!["plugin", "install", "acme/notes"];
        line.extend(args);
        match parse(&line).expect("parse").command {
            Some(Cli::Plugin(plugin)) => match plugin.action {
                PluginAction::Install(args) => args,
                _ => panic!("not install"),
            },
            _ => panic!("not plugin"),
        }
    }

    #[test]
    fn an_install_or_update_previews_without_asking_and_takes_the_commit_it_showed() {
        let args = install(&["--preview", "--json"]);
        assert!(args.preview && args.json && !args.yes && args.commit.is_none());
        let args = install(&["--commit", &SHA.to_ascii_uppercase(), "--yes"]);
        assert_eq!(args.commit.as_deref(), Some(SHA));
        for bad in [
            vec!["install", "a/b", "--preview", "--yes"],
            vec!["install", "a/b", "--json"],
            vec!["install", "a/b", "--commit", "0123abc"],
            vec!["install", "a/b", "--commit", SHA, "--preview"],
            vec!["update", "--preview", "--json"],
            vec!["update", "--commit", SHA],
        ] {
            assert!(!parses(&bad), "{bad:?}");
        }
        assert!(parses(&["update", "--preview"]) && parses(&["update", "acme.notes", "--preview", "--json"]));
    }

    #[test]
    fn another_commit_than_the_one_named_is_refused() {
        let (a, b) = (SHA, "fedcba9876543210fedcba9876543210fedcba98");
        assert!(same_commit(a, None, "acme/notes", "nothing was installed").is_ok());
        assert!(same_commit(a, Some(&a.to_ascii_uppercase()), "acme/notes", "nothing was installed").is_ok());
        let error = same_commit(a, Some(b), "acme/notes", "nothing was installed").expect_err("refused").to_string();
        assert!(
            error.contains(&a[..12]) && error.contains(&b[..12]) && error.contains("nothing was installed"),
            "{error}"
        );
    }
}

mod folders {
    use rstest::rstest;

    use super::reach::path_on;
    use super::*;
    use crate::test_util::TempDir;

    fn home_with_tools() -> (TempDir, String) {
        let home = TempDir::new();
        std::fs::create_dir(home.path().join("tools.git")).expect("create the folder");
        let expected = format!("{}//notes@v1", home.path().join("tools.git").display());
        (home, expected)
    }

    fn spec(typed: &str, home: Option<&Path>) -> String {
        folder_spec_from(typed, home).expect("a spec")
    }

    #[test]
    fn one_in_the_home_folder_becomes_absolute_and_keeps_its_subfolder_and_ref() {
        let (home, expected) = home_with_tools();

        assert_eq!(spec("~/tools.git//notes@v1", Some(home.path())), expected);
    }

    #[test]
    fn owner_and_repo_is_left_as_it_is() {
        assert_eq!(spec("acme/notes@v1", None), "acme/notes@v1");
    }

    #[cfg(windows)]
    #[test]
    fn one_holding_an_escape_character_comes_back_escaped() {
        let home = TempDir::new();

        let refused = folder_spec_from("~\\tools\u{1b}[2J.git", Some(home.path())).expect_err("refused").to_string();

        assert!(refused.contains(r"tools\u{1b}[2J.git") && !refused.contains('\u{1b}'), "{refused}");
    }

    #[cfg(windows)]
    #[test]
    fn a_missing_one_from_the_root_of_the_drive_becomes_absolute_for_git_to_report() {
        let typed = r"\cornercase.invalid\tools.git";
        let expected = std::path::absolute(typed).expect("absolute");

        let origin = Origin::parse(&spec(&format!("{typed}//notes@v1"), None)).expect("a folder");

        assert_eq!((Path::new(&origin.url), origin.local()), (expected.as_path(), true));
        assert_eq!(origin.subdir.as_deref(), Some("notes"));
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_path_and_one_in_the_home_folder_with_backslashes_become_canonical() {
        let (home, expected) = home_with_tools();
        let typed = format!(r"{}\tools.git\..\tools.git//notes@v1", home.path().display());

        assert_eq!(spec(r"~\tools.git//notes@v1", Some(home.path())), expected);
        assert_eq!(spec(&typed, None), expected);
        assert!(Origin::parse(&expected).expect("a folder").local());
    }

    #[cfg(windows)]
    #[test]
    fn a_network_folder_is_left_for_the_source_to_refuse() {
        let share = r"\\host\share\tools.git";

        assert_eq!(spec(share, None), share);
        assert!(Origin::parse(share).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_that_leads_to_the_network_through_a_link_is_refused_before_it_is_canonicalised() {
        let home = TempDir::new();
        let link = home.path().join("shared");
        crate::os::fs::make_junction(&link, Path::new(r"UNC\cornercase.invalid\share")).expect("a junction");

        let refused = folder_spec_from(r"~\shared\tools.git", Some(home.path())).expect_err("refused").to_string();

        assert!(refused.contains("cornercase.invalid") && refused.contains("on the network"), "{refused}");
    }

    #[rstest]
    #[case::the_folder_alone("~/tools", "~/tools", "")]
    #[case::with_a_subfolder_and_a_ref("~/tools//plugins/x@v1", "~/tools", "//plugins/x@v1")]
    #[case::with_a_ref("/srv/t@main", "/srv/t", "@main")]
    #[case::a_ref_is_split_first("/srv/a@b//c", "/srv/a", "@b//c")]
    #[case::a_drive_path(r"C:\code\tools//plugins/x", r"C:\code\tools", "//plugins/x")]
    fn the_folder_part_of_a_source(#[case] spec: &str, #[case] folder: &str, #[case] rest: &str) {
        assert_eq!(folder_part(spec), (folder, rest));
    }

    #[test]
    fn a_linked_folder_in_the_home_folder_is_expanded() {
        let home = TempDir::new();
        let home = home.path();

        assert_eq!(in_home(Path::new("~"), Some(home)), home);
        assert_eq!(in_home(Path::new("~/tools"), Some(home)), home.join("tools"));
        for kept in ["~user/x", "/abs", "./~/x"] {
            assert_eq!(in_home(Path::new(kept), Some(home)), Path::new(kept));
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_linked_folder_in_the_home_folder_with_backslashes_is_expanded() {
        let home = TempDir::new();

        assert_eq!(in_home(Path::new(r"~\tools"), Some(home.path())), home.path().join("tools"));
    }

    fn devbox() -> Remote {
        Remote { destination: "devbox".into(), command: None, option: "--command" }
    }

    fn unreachable_folder(_: &Path) -> Result<PathBuf> {
        unreachable!("the source names no folder")
    }

    #[rstest]
    #[case::a_repository("acme/notes")]
    #[case::a_pinned_folder_of_one("acme/tools/plugins/notes@v1")]
    #[case::an_address("https://git.example.com/acme/tools.git//plugins/notes@v2")]
    #[case::a_share("//server/share/tools")]
    fn a_source_that_is_no_folder_goes_as_it_is(#[case] spec: &str) {
        assert_eq!(source_on(spec, &unreachable_folder).expect("kept"), spec);
    }

    #[rstest]
    #[case::from_unix("/srv/tools//plugins/x@v1", false, false, "/srv/tools//plugins/x@v1")]
    #[case::backslashes_from_windows(r"/srv\tools//plugins/x", true, false, "/srv/tools//plugins/x")]
    #[case::a_windows_machine(r"C:\code\tools//plugins/x@v1", false, true, r"C:\code\tools//plugins/x@v1")]
    #[case::slashes_for_a_windows_machine("C:/code/tools//plugins/x", true, true, r"C:\code\tools//plugins/x")]
    fn a_folder_keeps_its_subfolder_and_its_ref(
        #[case] spec: &str,
        #[case] windows: bool,
        #[case] far: bool,
        #[case] sent: &str,
    ) {
        let remote = devbox();

        let said = source_on(spec, &|folder| path_on(folder, Some(&remote), windows, &|| far));

        assert_eq!(said.expect("kept"), sent);
    }

    #[test]
    fn a_windows_folder_for_a_unix_machine_is_refused() {
        let remote = devbox();

        let error = source_on(r"C:\code\tools", &|folder| path_on(folder, Some(&remote), true, &|| false))
            .expect_err("refused");

        assert!(error.to_string().contains("MSYS_NO_PATHCONV=1"), "{error}");
    }
}
