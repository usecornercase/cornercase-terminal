use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::test_util::TempDir;

fn plugin_in(dir: &Path, id: &str) {
    let text = format!("id = \"{id}\"\nname = \"Notes\"\nversion = \"1.0\"\n");
    std::fs::write(dir.join(manifest::FILE), text).expect("manifest");
}

fn linked(home: &TempDir, folder: &TempDir) -> Registry {
    plugin_in(folder.path(), "acme.notes");
    let mut registry = Registry::load(&home.path().join("plugins.json"));
    registry.link(folder.path()).expect("link");
    registry
}

#[test]
fn a_linked_plugin_starts_off_and_is_saved_privately() {
    let (home, folder) = (TempDir::new(), TempDir::new());
    plugin_in(folder.path(), "acme.notes");
    let path = home.path().join("plugins.json");
    let mut registry = Registry::load(&path);
    registry.link(folder.path()).expect("link");
    registry.save().expect("save");
    let mode = std::fs::metadata(&path).expect("saved").permissions().mode() & 0o777;
    let again = Registry::load(&path);
    assert_eq!((mode, again.entries.len(), again.entries[0].enabled), (0o600, 1, false));
}

#[test]
fn linking_the_same_folder_again_changes_nothing() {
    let (home, folder) = (TempDir::new(), TempDir::new());
    let mut registry = linked(&home, &folder);
    registry.set_enabled("acme.notes", true).expect("on");
    let (_, enabled) = registry.link(folder.path()).expect("link again");
    assert_eq!((registry.entries.len(), registry.entries[0].enabled, enabled), (1, true, true));
}

#[test]
fn another_folder_cannot_take_an_id_that_is_linked() {
    let (home, first, second) = (TempDir::new(), TempDir::new(), TempDir::new());
    let mut registry = linked(&home, &first);
    plugin_in(second.path(), "acme.notes");
    assert!(registry.link(second.path()).expect_err("refused").contains("already linked from"));
}

#[test]
fn a_plugin_that_cannot_run_cannot_be_turned_on() {
    let (home, folder) = (TempDir::new(), TempDir::new());
    let mut registry = linked(&home, &folder);
    std::fs::write(folder.path().join(manifest::FILE), "id = 3").expect("break it");
    assert!(registry.set_enabled("acme.notes", true).expect_err("refused").contains("cannot run"));
    assert!(registry.set_enabled("acme.notes", false).is_ok());
}

#[test]
fn a_broken_manifest_never_hides_the_others() {
    let (home, good, bad) = (TempDir::new(), TempDir::new(), TempDir::new());
    plugin_in(good.path(), "acme.good");
    plugin_in(bad.path(), "acme.bad");
    let mut registry = Registry::load(&home.path().join("plugins.json"));
    registry.link(good.path()).expect("link");
    registry.link(bad.path()).expect("link");
    std::fs::write(bad.path().join(manifest::FILE), "nonsense").expect("break it");
    let plugins = registry.plugins();
    assert_eq!((plugins[0].manifest.is_ok(), plugins[1].manifest.is_err()), (true, true));
}

#[test]
fn a_corrupt_or_unknown_registry_reads_as_empty() {
    let home = TempDir::new();
    let path = home.path().join("plugins.json");
    std::fs::write(&path, "{").expect("corrupt");
    assert_eq!(Registry::load(&path).entries, []);
    std::fs::write(&path, r#"{"version":9,"plugins":[]}"#).expect("unknown");
    assert_eq!(Registry::load(&path).entries, []);
}

#[test]
fn the_watch_answers_once_per_change() {
    let (home, folder) = (TempDir::new(), TempDir::new());
    plugin_in(folder.path(), "acme.notes");
    let path = home.path().join("plugins.json");
    let mut watch = Watch::new(path.clone());
    assert_eq!(watch.changed().map(|p| p.len()), Some(0));
    assert_eq!(watch.changed(), None);
    let mut registry = Registry::load(&path);
    registry.link(folder.path()).expect("link");
    registry.save().expect("save");
    assert_eq!(watch.changed().map(|p| p.len()), Some(1));
    assert_eq!(watch.changed(), None);
    std::fs::write(folder.path().join(manifest::FILE), "id = 3").expect("edit the manifest");
    assert!(watch.changed().is_some_and(|p| p[0].manifest.is_err()));
}

#[test]
fn a_registry_that_cannot_be_read_is_never_written_over() {
    let home = TempDir::new();
    let path = home.path().join("plugins.json");
    assert_eq!(Registry::open(&path).expect("missing is empty").entries, []);
    std::fs::write(&path, "{").expect("corrupt");
    assert!(Registry::open(&path).expect_err("refused").contains("will not write over it"));
    std::fs::write(&path, r#"{"version":9,"plugins":[]}"#).expect("newer");
    assert!(Registry::open(&path).expect_err("refused").contains("version 9"));
}

#[test]
fn a_link_path_is_shown_with_its_invisible_characters_escaped() {
    let source = Source::Link { path: PathBuf::from("/plugins/evil\u{202e}lmth.sh") };
    assert_eq!(source.describe(), "linked from /plugins/evil\\u{202e}lmth.sh");
}

#[test]
fn a_folder_that_cannot_be_linked_is_named_with_its_invisible_characters_escaped() {
    let home = TempDir::new();
    let mut registry = Registry::load(&home.path().join("plugins.json"));
    let error = registry.link(&home.path().join("gone\u{202e}")).expect_err("refused");
    assert!(error.contains("gone\\u{202e}") && !error.contains('\u{202e}'), "{error}");
}

mod programs {
    use std::os::unix::fs::symlink;

    use super::*;

    fn with_service(dir: &Path) {
        let text = "id = \"acme.notes\"\nname = \"Notes\"\nversion = \"1.0\"\n\n[[service]]\nid = \"s\"\ncommand = [\"bin/run\"]\n";
        std::fs::write(dir.join(manifest::FILE), text).expect("manifest");
        std::fs::create_dir_all(dir.join("bin")).expect("bin");
    }

    #[test]
    fn a_program_linked_out_of_the_folder_cannot_be_linked() {
        let (home, folder) = (TempDir::new(), TempDir::new());
        with_service(folder.path());
        symlink("/bin/sh", folder.path().join("bin/run")).expect("symlink");
        let error = Registry::load(&home.path().join("plugins.json")).link(folder.path()).expect_err("refused");
        assert!(error.contains("service s: `bin/run` is outside the plugin's folder"), "{error}");
    }

    #[test]
    fn a_program_linked_out_later_cannot_run_and_says_why() {
        let (home, folder) = (TempDir::new(), TempDir::new());
        with_service(folder.path());
        std::fs::write(folder.path().join("bin/run"), "#!/bin/sh\n").expect("script");
        let mut registry = Registry::load(&home.path().join("plugins.json"));
        registry.link(folder.path()).expect("link");
        std::fs::remove_file(folder.path().join("bin/run")).expect("remove");
        symlink("/bin/sh", folder.path().join("bin/run")).expect("symlink");
        let listed = registry.plugins()[0].manifest.clone().expect_err("cannot run");
        assert!(listed.contains("outside the plugin's folder"), "{listed}");
        assert!(registry.set_enabled("acme.notes", true).expect_err("refused").contains("outside"));
    }
}

#[test]
fn the_label_and_note_are_what_settings_shows() {
    let (home, folder) = (TempDir::new(), TempDir::new());
    let mut registry = linked(&home, &folder);
    let off = registry.plugins().remove(0);
    assert_eq!((off.label().as_str(), off.note(None).as_str()), ("Notes 1.0", "declares no permissions · runs as you"));
    registry.set_enabled("acme.notes", true).expect("on");
    let on = registry.plugins().remove(0);
    assert_eq!(on.note(None), "");
    assert_eq!(on.note(Some("service sync runs")), "service sync runs");
    assert_eq!(off.note(Some("service sync runs")), "declares no permissions · runs as you");
    std::fs::write(folder.path().join(manifest::FILE), "broken").expect("manifest");
    let broken = registry.plugins().remove(0);
    assert_eq!(broken.label(), "acme.notes");
    assert_ne!(broken.note(None), "");
}
