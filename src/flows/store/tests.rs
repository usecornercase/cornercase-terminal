use rstest::rstest;

use super::*;
use crate::agents::Known;
use crate::flows::builtin;
use crate::test_util::TempDir;

fn named(name: &str) -> Flow {
    Flow { name: name.to_string(), ..builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0) }
}

mod slugs {
    use super::*;

    #[test]
    fn file_names_are_slugs() {
        assert_eq!(file_name("Bug, test first"), "bug-test-first");
        assert_eq!(file_name("  ¡Revisión!  "), "revisión");
        assert_eq!(file_name("***"), "flow");
    }

    #[rstest]
    #[case::latin("a")]
    #[case::cjk("流")]
    #[case::four_bytes("𠀀")]
    fn a_long_slug_is_cut_after_the_last_whole_character_that_fits(#[case] letter: &str) {
        assert_eq!(file_name(&letter.repeat(300)), letter.repeat(MAX_SLUG / letter.len()));
    }

    #[test]
    fn a_cut_that_ends_on_a_dash_drops_it() {
        let name = format!("{} {}", "a".repeat(MAX_SLUG - 1), "b".repeat(50));
        assert_eq!(file_name(&name), "a".repeat(MAX_SLUG - 1));
    }

    #[test]
    fn the_longest_file_a_save_makes_is_the_longest_name_persist_writes() {
        let longest = format!("{}-{MAX_COPIES}.{EXTENSION}", "a".repeat(MAX_SLUG));
        assert_eq!(longest.len(), persist::MAX_NAME);
    }
}

mod saving {
    use super::*;

    #[rstest]
    #[case::latin("a".repeat(300))]
    #[case::cjk("流程".repeat(40))]
    fn a_long_name_saves_under_a_cut_file_name_and_reads_back_whole(#[case] name: String) {
        let dir = TempDir::new();
        let path = save(dir.path(), &named(&name), None).expect("save");
        let file = path.file_name().and_then(std::ffi::OsStr::to_str).expect("a utf-8 name");
        assert!(file.len() <= persist::MAX_NAME);
        assert_eq!(read(&path).expect("read").name, name);
    }

    #[test]
    fn two_long_names_that_share_the_cut_slug_save_as_two_files() {
        let dir = TempDir::new();
        let long = "a".repeat(300);
        let file = |stem: &str| dir.path().join(format!("{stem}.{EXTENSION}"));
        let one = save(dir.path(), &named(&format!("{long} one")), None).expect("save one");
        let two = save(dir.path(), &named(&format!("{long} two")), None).expect("save two");
        let slug = "a".repeat(MAX_SLUG);
        assert_eq!(one, file(&slug));
        assert_eq!(two, file(&format!("{slug}-2")));
        let listing = list(Vec::new(), Chosen::default(), dir.path(), None);
        let names: Vec<_> = listing.flows.iter().map(|saved| saved.flow.name.clone()).collect();
        assert_eq!(names, [format!("{long} one"), format!("{long} two")]);
    }

    #[test]
    fn a_file_saved_under_a_longer_slug_before_moves_to_the_cut_one() {
        let dir = TempDir::new();
        let flow = named(&"a".repeat(240));
        let old = dir.path().join(format!("{}.{EXTENSION}", "a".repeat(240)));
        std::fs::write(&old, to_text(&flow).expect("text")).expect("write the old file");
        let path = save(dir.path(), &flow, Some(&old)).expect("save");
        assert_eq!(path, dir.path().join(format!("{}.{EXTENSION}", "a".repeat(MAX_SLUG))));
        assert!(!old.exists());
    }
}
