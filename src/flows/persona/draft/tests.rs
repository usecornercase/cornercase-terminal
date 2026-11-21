use super::*;
use crate::flows::persona::parse;

const IMPORTED: &str = "---
name: Code Reviewer
description: |
  Reviews code.
color: purple
vibe: Like a mentor.
source: \"owner/repo/reviewer.md\"
---

# Code Reviewer

Review it.
";

mod composing {
    use super::*;

    #[test]
    fn a_new_persona_has_its_fields_and_body_and_no_source() {
        let draft = Draft {
            name: "Tidy \"Tester\"".into(),
            description: "Tests things.".into(),
            emoji: "🧪".into(),
            body: "Write a test first.\r\nThen fix it.".into(),
        };

        let text = compose(&draft, None);

        let persona = parse("tidy-tester", &text).expect("a persona");
        assert_eq!(persona.name, "Tidy \"Tester\"");
        assert_eq!((persona.description.as_str(), persona.emoji.as_str()), ("Tests things.", "🧪"));
        assert_eq!(persona.source, None);
        assert_eq!(body(&text), "Write a test first.\nThen fix it.\n");
    }

    #[test]
    fn an_edit_keeps_the_other_keys_and_drops_the_source() {
        let draft = Draft {
            name: "Reviewer".into(),
            description: String::new(),
            emoji: "👁️".into(),
            body: body(IMPORTED).replace("Review it.", "Review it twice."),
        };

        let text = compose(&draft, Some(IMPORTED));

        let persona = parse("reviewer", &text).expect("a persona");
        assert_eq!((persona.name.as_str(), persona.description.as_str()), ("Reviewer", ""));
        assert_eq!((persona.color.as_str(), persona.source), ("purple", None));
        assert!(text.contains("vibe: Like a mentor.\n"), "{text}");
        assert!(!text.contains("Reviews code."), "{text}");
        assert!(text.ends_with("---\n\n# Code Reviewer\n\nReview it twice.\n"), "{text}");
    }
}

mod the_body {
    use super::*;

    #[rstest::rstest]
    #[case::after_the_front_matter(IMPORTED, "# Code Reviewer\n\nReview it.\n")]
    #[case::without_one("Just text.\n", "Just text.\n")]
    #[case::with_a_byte_order_mark("\u{feff}---\r\nname: X\r\n---\r\n\r\nBody.\r\n", "Body.\r\n")]
    fn is_the_text_after_the_front_matter(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(body(text), expected);
    }
}
