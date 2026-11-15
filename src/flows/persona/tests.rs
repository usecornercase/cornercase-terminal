use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

const REVIEWER: &str = "---
name: Code Reviewer
description: Expert code reviewer who gives actionable feedback.
color: purple
emoji: 👁️
vibe: Reviews code like a mentor, not a gatekeeper.
---

# Code Reviewer Agent

You are **Code Reviewer**.
";

mod reading {
    use super::*;

    #[test]
    fn takes_the_front_matter_and_keeps_the_text_whole() {
        let persona = parse("engineering-code-reviewer", REVIEWER).expect("a persona");

        assert_eq!(persona.id, "engineering-code-reviewer");
        assert_eq!(persona.name, "Code Reviewer");
        assert_eq!(persona.description, "Expert code reviewer who gives actionable feedback.");
        assert_eq!((persona.emoji.as_str(), persona.color.as_str()), ("👁️", "purple"));
        assert_eq!(persona.source, None);
        assert_eq!(persona.text, REVIEWER);
    }

    #[test]
    fn quoted_values_and_folded_blocks_are_read() {
        let text =
            "---\nname: \"The \\\"Fixer\\\"\"\ndescription: >\n  Fixes things\n  that break.\nemoji: '🔧'\n---\nbody\n";

        let persona = parse("fixer", text).expect("a persona");

        assert_eq!(persona.name, "The \"Fixer\"");
        assert_eq!(persona.description, "Fixes things that break.");
        assert_eq!(persona.emoji, "🔧");
    }

    #[test]
    fn windows_line_ends_and_a_byte_order_mark_are_taken() {
        let text = "\u{feff}---\r\nname: Tester\r\n---\r\nTest it.\r\n";

        let persona = parse("tester", text).expect("a persona");

        assert_eq!(persona.name, "Tester");
        assert!(persona.text.starts_with("---\r\n"), "{:?}", persona.text);
    }

    #[rstest]
    #[case::no_front_matter("# Reviewer\n", "it has no front matter")]
    #[case::unclosed("---\nname: Reviewer\n", "it has no front matter")]
    #[case::no_name("---\ndescription: x\n---\n", "its front matter gives no `name`")]
    #[case::empty_name("---\nname:   \n---\n", "its front matter gives no `name`")]
    #[case::control("---\nname: Re\u{1b}[31mviewer\n---\n", "contains a control character")]
    fn what_is_not_a_persona_says_why(#[case] text: &str, #[case] reason: &str) {
        let error = parse("reviewer", text).expect_err("not a persona");
        assert!(error.contains(reason), "{error}");
        assert!(!is_persona(text) || reason.contains("control"), "{text:?}");
    }

    #[test]
    fn a_long_text_is_refused() {
        let text = format!("---\nname: Long\n---\n{}", "a".repeat(MAX_BYTES));
        assert!(parse("long", &text).expect_err("too long").contains("at most 256 KiB"));
    }
}

mod ids {
    use super::*;

    #[rstest]
    #[case("engineering-code-reviewer", true)]
    #[case("a1", true)]
    #[case("", false)]
    #[case("Upper", false)]
    #[case("-lead", false)]
    #[case("trail-", false)]
    #[case("dots.md", false)]
    #[case("../up", false)]
    fn are_lower_case_letters_digits_and_dashes(#[case] id: &str, #[case] valid: bool) {
        assert_eq!(valid_id(id), valid);
    }

    #[rstest]
    #[case::from_the_file("engineering/engineering-code-reviewer.md", "Code Reviewer", "engineering-code-reviewer")]
    #[case::from_a_messy_file("My Reviewer (v2).MD", "x", "my-reviewer-v2")]
    #[case::from_the_name("", "Code Reviewer", "code-reviewer")]
    #[case::fallback("", "👁️", "persona")]
    fn come_from_the_file_name_then_the_name(#[case] file: &str, #[case] name: &str, #[case] id: &str) {
        assert_eq!(id_of(file, name), id);
    }
}

mod sources {
    use super::*;

    #[test]
    fn are_added_to_the_front_matter() {
        let text = with_source(REVIEWER, "msitarzewski/agency-agents/engineering/engineering-code-reviewer.md");

        let persona = parse("engineering-code-reviewer", &text).expect("a persona");

        assert_eq!(
            persona.source.as_deref(),
            Some("msitarzewski/agency-agents/engineering/engineering-code-reviewer.md")
        );
        assert!(
            text.contains("vibe: Reviews code like a mentor, not a gatekeeper.\nsource: \"msitarzewski/"),
            "{text}"
        );
        assert!(text.ends_with("---\n\n# Code Reviewer Agent\n\nYou are **Code Reviewer**.\n"), "{text}");
    }

    #[test]
    fn replace_the_one_before_and_are_quoted() {
        let once = with_source(REVIEWER, "C:\\old \"place\"");
        let twice = with_source(&once, "https://example.com/r.md");

        assert_eq!(parse("r", &once).expect("once").source.as_deref(), Some("C:\\old \"place\""));
        assert_eq!(twice.matches("source:").count(), 1, "{twice}");
        assert_eq!(parse("r", &twice).expect("twice").source.as_deref(), Some("https://example.com/r.md"));
    }

    #[test]
    fn an_import_takes_its_id_from_the_file_and_its_source() {
        let persona = imported("reviewer.md", REVIEWER, Some("/home/me/reviewer.md")).expect("a persona");

        assert_eq!(persona.id, "reviewer");
        assert_eq!(persona.source.as_deref(), Some("/home/me/reviewer.md"));
    }
}

mod the_library {
    use super::*;

    fn saved(dir: &Path, id: &str, text: &str) {
        save(dir, &parse(id, text).expect("a persona")).expect("saved");
    }

    #[test]
    fn lists_by_name_and_names_what_it_cannot_read() {
        let dir = TempDir::new();
        saved(dir.path(), "zed", "---\nname: Alpha\n---\n");
        saved(dir.path(), "abe", "---\nname: Beta\n---\n");
        std::fs::write(dir.path().join("broken.md"), "no front matter").expect("write");
        std::fs::write(dir.path().join("Not An Id.md"), "---\nname: Skipped\n---\n").expect("write");
        std::fs::write(dir.path().join("notes.txt"), "---\nname: Skipped\n---\n").expect("write");

        let listing = list(dir.path());

        let names: Vec<(&str, &str)> = listing.personas.iter().map(|p| (p.id.as_str(), p.name.as_str())).collect();
        assert_eq!(names, [("zed", "Alpha"), ("abe", "Beta")]);
        assert_eq!(listing.broken.len(), 1);
        assert!(listing.broken[0].0.ends_with("broken.md"), "{:?}", listing.broken);
        assert_eq!(ids(dir.path()), ["abe", "broken", "zed"]);
    }

    #[test]
    fn reads_saves_and_removes_one_by_its_id() {
        let dir = TempDir::new();
        saved(dir.path(), "engineering-code-reviewer", REVIEWER);

        assert_eq!(read(dir.path(), "engineering-code-reviewer").expect("read").name, "Code Reviewer");
        let path = remove(dir.path(), "engineering-code-reviewer").expect("removed");
        assert!(!path.exists());
        assert!(remove(dir.path(), "engineering-code-reviewer").is_err());
        assert!(read(dir.path(), "../config").expect_err("not an id").contains("not a persona id"));
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        let dir = TempDir::new();
        assert_eq!(list(&dir.path().join("personas")), Listing::default());
        assert_eq!(ids(&dir.path().join("personas")), Vec::<String>::new());
    }
}
