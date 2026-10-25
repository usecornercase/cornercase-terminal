use std::os::unix::fs::symlink;

use super::*;
use crate::test_util::TempDir;

const FULL: &str = r#"
id = "acme.notes"
name = "Notes"
version = "1.2.0"
min_cornercase = "0.1.0"
description = "Notes for every workspace"
permissions = ["read", "send"]
somewhere_later = "is ignored"

[[actions]]
id = "open"
title = "open notes"
contexts = ["workspace", "pane"]
command = ["bin/notes", "open"]

[[events]]
on = "status"
command = ["bin/notes", "status"]

[[service]]
id = "sync"
command = ["bin/notes", "sync"]

[[window]]
id = "panel"
command = ["bin/notes", "panel"]
"#;

fn with(extra: &str) -> String {
    format!("id = \"acme.notes\"\nname = \"Notes\"\nversion = \"1\"\n{extra}")
}

mod parsing {
    use super::*;

    #[test]
    fn a_full_manifest_reads_every_entry() {
        let manifest = Manifest::parse(FULL).expect("parse");
        assert_eq!(
            (
                manifest.actions[0].contexts.clone(),
                manifest.events[0].on,
                manifest.service[0].id.as_str(),
                manifest.window[0].command.clone(),
                manifest.api,
            ),
            (vec![Context::Workspace, Context::Pane], On::Status, "sync", vec!["bin/notes".into(), "panel".into()], 1)
        );
    }

    #[test]
    fn the_commands_and_their_fields_keep_the_preview_order() {
        let manifest = Manifest::parse(FULL).expect("parse");
        let labels: Vec<String> = manifest.commands().into_iter().map(|(label, _)| label).collect();
        assert_eq!(labels, ["service sync", "window process panel", "action open (workspace, pane)", "on status"]);
        assert_eq!(
            serde_json::to_value(manifest.declared()).expect("json"),
            serde_json::json!([
                {"entry": "service", "id": "sync", "command": ["bin/notes", "sync"]},
                {"entry": "window", "id": "panel", "command": ["bin/notes", "panel"]},
                {"entry": "action", "id": "open", "title": "open notes", "contexts": ["workspace", "pane"],
                 "command": ["bin/notes", "open"]},
                {"entry": "hook", "on": "status", "command": ["bin/notes", "status"]},
            ])
        );
    }

    #[test]
    fn it_says_what_it_runs_and_asks_for() {
        let manifest = Manifest::parse(FULL).expect("parse");
        assert_eq!(
            (manifest.runs(), manifest.asks()),
            (
                vec!["1 service".into(), "1 window process".into(), "1 action".into(), "1 event hook".into()],
                "asks for read, send".into()
            )
        );
    }

    #[rstest::rstest]
    #[case::an_id_with_capitals("id = \"Acme\"\nname = \"x\"\nversion = \"1\"", "must be lower-case")]
    #[case::an_id_with_a_slash("id = \"a/b\"\nname = \"x\"\nversion = \"1\"", "must be lower-case")]
    #[case::no_name("id = \"a\"\nname = \" \"\nversion = \"1\"", "`name` is empty")]
    #[case::a_missing_field("id = \"a\"\nname = \"x\"", "missing field `version`")]
    #[case::a_newer_cornercase(&with("min_cornercase = \"999.0.0\""), "needs cornercase 999.0.0")]
    #[case::a_newer_api(&with("api = 2"), "needs plugin API 2")]
    #[case::another_platform(&with("platforms = [\"plan9\"]"), "runs on plan9 only")]
    #[case::an_unknown_context(&with("[[actions]]\nid = \"a\"\ntitle = \"t\"\ncontexts = [\"group\"]\ncommand = [\"x\"]"), "unknown variant `group`")]
    #[case::no_context(&with("[[actions]]\nid = \"a\"\ntitle = \"t\"\ncontexts = []\ncommand = [\"x\"]"), "at least one context")]
    #[case::an_unknown_event(&with("[[events]]\non = \"typed\"\ncommand = [\"x\"]"), "unknown variant `typed`")]
    #[case::an_empty_command(&with("[[service]]\nid = \"s\"\ncommand = []"), "has no command")]
    #[case::a_command_leaving_the_folder(&with("[[service]]\nid = \"s\"\ncommand = [\"../bin/x\"]"), "outside the plugin's folder")]
    #[case::a_nul_in_an_argument(&with("[[service]]\nid = \"s\"\ncommand = [\"x\", \"a\\u0000b\"]"), "invisible or control character")]
    #[case::two_services_with_one_id(&with("[[service]]\nid = \"s\"\ncommand = [\"x\"]\n[[service]]\nid = \"s\"\ncommand = [\"y\"]"), "two service entries")]
    #[case::a_bidi_override_in_a_command(&with("[[service]]\nid = \"s\"\ncommand = [\"bin/run\\u202e\"]"), "`bin/run\\u{202e}` has an invisible")]
    #[case::an_isolate_in_an_argument(&with("[[service]]\nid = \"s\"\ncommand = [\"x\", \"a\\u2066b\"]"), "a command `a\\u{2066}b`")]
    #[case::a_zero_width_space_in_the_name(
        "id = \"a\"\nname = \"No\\u200Btes\"\nversion = \"1\"",
        "name `No\\u{200b}tes`"
    )]
    #[case::a_line_separator_in_the_description(&with("description = \"a\\u2028b\""), "description")]
    #[case::a_byte_order_mark_in_a_permission(&with("permissions = [\"\\uFEFFsend\"]"), "permissions")]
    #[case::a_bidi_override_in_a_build_step(&with("[[build]]\ncommand = [\"make\", \"a\\u202eb\"]"), "a build command `a\\u{202e}b`")]
    #[case::a_bidi_override_in_a_download(&with("[download.x86_64-unknown-linux-gnu]\nurl = \"https://acme.dev/a\\u202eb\"\nsha256 = \"00\""), "a download url")]
    #[case::a_soft_hyphen_in_a_title(&with("[[actions]]\nid = \"a\"\ntitle = \"t\\u00ADx\"\ncontexts = [\"tab\"]\ncommand = [\"x\"]"), "an action title")]
    #[case::a_newline_in_the_version("id = \"a\"\nname = \"x\"\nversion = \"1\\n2\"", "version `1\\u{a}2`")]
    #[case::a_tag_in_a_hook(&with("[[events]]\non = \"opened\"\ncommand = [\"x\\U000E0041\"]"), "a command")]
    #[case::a_bidi_override_in_an_unknown_variant(&with("[[events]]\non = \"op\\u202eened\"\ncommand = [\"x\"]"), "op\\u{202e}ened")]
    fn it_refuses(#[case] text: &str, #[case] reason: &str) {
        let error = Manifest::parse(text).expect_err("refused");
        assert!(error.contains(reason), "{error}");
    }

    #[test]
    fn a_long_text_is_quoted_around_its_invisible_character() {
        let description = format!("{}\u{202e}{}", "a".repeat(500), "b".repeat(500));
        let error = Manifest::parse(&with(&format!("description = \"{description}\""))).expect_err("refused");
        assert!(error.contains("…aaa") && error.contains("\\u{202e}bbb") && error.len() < 200, "{error}");
    }

    #[test]
    fn it_refuses_too_many_entries() {
        let many = "[[service]]\nid = \"s\"\ncommand = [\"x\"]\n".repeat(65);
        assert!(Manifest::parse(&with(&many)).expect_err("refused").contains("more than the 64"));
    }

    #[test]
    fn the_platform_it_runs_on_is_accepted_in_any_case() {
        let text = with(&format!("platforms = [\"{}\"]", platform().to_uppercase()));
        assert!(Manifest::parse(&text).is_ok());
    }
}

mod buttons {
    use super::*;

    const BUTTON: &str = "[button]\ntitle = \"Deploy\"\nicon = \"D\"\n";
    const TAB_ACTION: &str = "[[actions]]\nid = \"open\"\ntitle = \"o\"\ncontexts = [\"tab\"]\ncommand = [\"x\"]\n";

    fn action(extra: &str) -> String {
        format!("[[actions]]\nid = \"status\"\ntitle = \"Deploy status\"\ncommand = [\"bin/status\"]\n{extra}")
    }

    fn popup(size: &str) -> String {
        with(&action(&format!("contexts = [\"button\"]\nplacement = \"popup\"\n{size}")))
    }

    #[test]
    fn a_button_with_a_sized_popup_is_read() {
        let text = with(&format!(
            "{BUTTON}{}",
            action("contexts = [\"button\", \"workspace\"]\nplacement = \"popup\"\nwidth = 100\nheight = 30")
        ));
        let manifest = Manifest::parse(&text).expect("parse");
        let status = &manifest.actions[0];
        assert_eq!(
            (status.contexts.clone(), status.placement, status.width, status.height, manifest.face()),
            (
                vec![Context::Button, Context::Workspace],
                Placement::Popup,
                Some(100),
                Some(30),
                Some(Face { title: "Deploy".into(), icon: "D".into() })
            )
        );
    }

    #[test]
    fn an_action_runs_as_a_job_unless_it_says_otherwise() {
        let manifest = Manifest::parse(FULL).expect("parse");
        assert_eq!((manifest.actions[0].placement, manifest.face()), (Placement::Job, None));
    }

    #[test]
    fn a_button_without_its_table_takes_the_name_and_its_initial() {
        let manifest = Manifest::parse(&with(&action("contexts = [\"button\"]"))).expect("parse");
        assert_eq!(manifest.face(), Some(Face { title: "Notes".into(), icon: "N".into() }));
    }

    #[test]
    fn the_button_lists_only_its_actions_in_order() {
        let second = action("contexts = [\"button\"]").replace("\"status\"", "\"logs\"");
        let text = with(&format!("{}\n{TAB_ACTION}{second}", action("contexts = [\"button\"]")));
        let manifest = Manifest::parse(&text).expect("parse");
        let ids: Vec<&str> = manifest.button_actions().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["status", "logs"]);
    }

    #[rstest::rstest]
    #[case::an_unknown_placement(&with(&action("contexts = [\"button\"]\nplacement = \"window\"")), "unknown variant `window`")]
    #[case::a_width_on_a_job(&with(&action("contexts = [\"tab\"]\nwidth = 80")), "only a popup has")]
    #[case::a_height_on_a_job(&with(&action("contexts = [\"tab\"]\nplacement = \"job\"\nheight = 20")), "only a popup has")]
    #[case::a_popup_too_narrow(&popup("width = 19"), "width of 19, not 20 to 500")]
    #[case::a_popup_too_wide(&popup("width = 501"), "width of 501, not 20 to 500")]
    #[case::a_popup_too_short(&popup("height = 4"), "height of 4, not 5 to 200")]
    #[case::a_popup_too_tall(&popup("height = 201"), "height of 201, not 5 to 200")]
    #[case::a_negative_width(&popup("width = -1"), "expected u16")]
    #[case::an_icon_of_two_characters(&with(&format!("[button]\nicon = \"De\"\n{}", action("contexts = [\"button\"]"))), "must be one character one cell wide")]
    #[case::a_wide_icon(&with(&format!("[button]\nicon = \"界\"\n{}", action("contexts = [\"button\"]"))), "must be one character one cell wide")]
    #[case::an_empty_icon(&with(&format!("[button]\nicon = \"\"\n{}", action("contexts = [\"button\"]"))), "must be one character one cell wide")]
    #[case::a_hidden_icon(&with(&format!("[button]\nicon = \"\\u200B\"\n{}", action("contexts = [\"button\"]"))), "the button's icon `\\u{200b}` has an invisible")]
    #[case::a_hidden_title(&with(&format!("[button]\ntitle = \"a\\u202eb\"\n{}", action("contexts = [\"button\"]"))), "the button's title")]
    #[case::an_empty_title(&with(&format!("[button]\ntitle = \" \"\n{}", action("contexts = [\"button\"]"))), "the button's `title` is empty")]
    #[case::a_button_without_button_actions(&with(&format!("{BUTTON}{}", action("contexts = [\"tab\"]"))), "needs an action with the `button` context")]
    #[case::a_button_without_actions(&with(BUTTON), "needs an action with the `button` context")]
    #[case::no_context_on_a_button_action(&with(&format!("{BUTTON}{}", action("contexts = []"))), "at least one context")]
    fn it_refuses(#[case] text: &str, #[case] reason: &str) {
        let error = Manifest::parse(text).expect_err("refused");
        assert!(error.contains(reason), "{error}");
    }
}

mod loading {
    use super::*;

    #[test]
    fn a_manifest_that_is_not_a_file_is_not_read() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join(FILE)).expect("folder in its place");
        assert!(Manifest::load(dir.path()).expect_err("refused").contains("not a file"));
    }

    #[test]
    fn a_huge_manifest_is_not_read() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(FILE), with(&"#".repeat(70_000))).expect("write");
        assert!(Manifest::load(dir.path()).expect_err("refused").contains("larger than 64 KiB"));
    }
}

mod programs {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn root_with_bin() -> TempDir {
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join("bin")).expect("bin");
        std::fs::write(dir.path().join("bin/run"), "").expect("program");
        dir
    }

    #[test]
    fn a_relative_path_runs_from_the_plugin_folder() {
        let dir = root_with_bin();
        let (program, args) = program(dir.path(), &["bin/run".into(), "now".into()]).expect("inside");
        assert_eq!((program, args), (dir.path().join("bin/run"), vec!["now".to_string()]));
    }

    #[test]
    fn a_bare_name_is_looked_up_on_the_path() {
        let (program, _) = program(Path::new("/nowhere"), &["sh".into()]).expect("a name");
        assert_eq!(program, PathBuf::from("sh"));
    }

    #[test]
    fn a_program_anyone_can_write_to_is_refused() {
        let dir = root_with_bin();
        std::fs::set_permissions(dir.path().join("bin/run"), Permissions::from_mode(0o757)).expect("chmod");
        let error = program(dir.path(), &["bin/run".into()]).expect_err("refused");
        assert!(error.contains("other users can write to `bin/run`"), "{error}");
    }

    #[test]
    fn a_folder_on_the_way_anyone_can_write_to_is_refused() {
        let dir = root_with_bin();
        std::fs::set_permissions(dir.path().join("bin"), Permissions::from_mode(0o777)).expect("chmod");
        let error = program(dir.path(), &["bin/run".into()]).expect_err("refused");
        assert!(error.contains("other users can write to `bin`"), "{error}");
    }

    #[test]
    fn a_program_another_group_can_write_to_is_refused() {
        let dir = root_with_bin();
        let path = dir.path().join("bin/run");
        let groups = std::process::Command::new("id").arg("-G").output().expect("id -G");
        let other = String::from_utf8_lossy(&groups.stdout)
            .split_whitespace()
            .filter_map(|group| group.parse::<u32>().ok())
            .find(|&group| group != getgid().as_raw());
        let Some(other) = other else { return };
        std::os::unix::fs::chown(&path, None, Some(other)).expect("chown to one of the user's groups");
        std::fs::set_permissions(&path, Permissions::from_mode(0o775)).expect("chmod");
        let error = program(dir.path(), &["bin/run".into()]).expect_err("refused");
        assert!(error.contains("other users can write to `bin/run`"), "{error}");
    }

    #[test]
    fn a_program_in_the_users_own_group_is_accepted() {
        let dir = root_with_bin();
        std::fs::set_permissions(dir.path().join("bin/run"), Permissions::from_mode(0o775)).expect("chmod");
        assert!(program(dir.path(), &["bin/run".into()]).is_ok());
    }

    #[test]
    fn a_symlink_out_of_the_folder_is_refused() {
        let dir = root_with_bin();
        symlink("/bin/sh", dir.path().join("bin/out")).expect("symlink");
        let error = program(dir.path(), &["bin/out".into()]).expect_err("refused");
        assert!(error.contains("outside the plugin's folder"), "{error}");
    }
}

mod pipes {
    use super::*;

    #[test]
    fn a_manifest_that_is_a_fifo_is_refused_without_waiting() {
        let dir = TempDir::new();
        let made = std::process::Command::new("mkfifo").arg(dir.path().join(FILE)).status().expect("mkfifo");
        assert!(made.success());
        assert!(Manifest::load(dir.path()).expect_err("refused").contains("not a file"));
    }
}

mod owners {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn a_folder_anyone_can_write_to_is_refused() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(FILE), with("")).expect("manifest");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).expect("chmod");
        assert!(
            Manifest::load(dir.path()).expect_err("refused").contains("other users can write to the plugin's folder")
        );
    }

    #[test]
    fn a_manifest_anyone_can_write_to_is_refused() {
        let dir = TempDir::new();
        let path = dir.path().join(FILE);
        std::fs::write(&path, with("")).expect("manifest");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).expect("chmod");
        assert!(
            Manifest::load(dir.path())
                .expect_err("refused")
                .contains("other users can write to cornercase-plugin.toml")
        );
    }
}

#[rstest::rstest]
#[case::an_escape_in_a_command("[[build]]\ncommand = [\"x\", \"\\u001b[6A\\u001b[J\"]")]
#[case::a_carriage_return_in_the_description("description = \"safe\\rhidden\"")]
#[case::a_new_line_in_a_permission("permissions = [\"read\\nsend\"]")]
fn a_control_character_that_could_rewrite_the_preview_is_refused(#[case] extra: &str) {
    let error = Manifest::parse(&with(extra)).expect_err("refused");
    assert!(error.contains("control character"), "{error}");
}
