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
    #[case::a_nul_in_an_argument(&with("[[service]]\nid = \"s\"\ncommand = [\"x\", \"a\\u0000b\"]"), "control character")]
    #[case::two_services_with_one_id(&with("[[service]]\nid = \"s\"\ncommand = [\"x\"]\n[[service]]\nid = \"s\"\ncommand = [\"y\"]"), "two service entries")]
    fn it_refuses(#[case] text: &str, #[case] reason: &str) {
        let error = Manifest::parse(text).expect_err("refused");
        assert!(error.contains(reason), "{error}");
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
