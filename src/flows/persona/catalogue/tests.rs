use super::*;
use crate::test_util::TempDir;

const REVIEWER: &str = "---\nname: Code Reviewer\n---\nReview it.\n";
const TESTER: &str = "---\nname: Tester\n---\nTest it.\n";

fn one<'a>(file: &'a str, text: &'a str) -> Incoming<'a> {
    Incoming { file, text, source: None }
}

fn problems(offered: &[Offer]) -> Vec<Option<&str>> {
    offered.iter().map(|offer| offer.problem.as_deref()).collect()
}

mod offers {
    use super::*;

    #[test]
    fn a_persona_that_fits_has_no_problem_and_says_whether_it_replaces_yours() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("reviewer.md"), REVIEWER).expect("write");
        let listing = crate::flows::persona::list(dir.path());

        let offered = offers(&[one("reviewer.md", REVIEWER), one("tester.md", TESTER)], (&listing, dir.path()));

        assert_eq!(problems(&offered), [None, None]);
        assert_eq!(offered.iter().map(|offer| offer.replaced).collect::<Vec<_>>(), [true, false]);
        assert_eq!(offered[1].persona.as_ref().map(|persona| persona.id.as_str()), Some("tester"));
    }

    #[test]
    fn each_problem_stays_on_its_own_persona() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("mine.md"), TESTER).expect("write");
        let listing = crate::flows::persona::list(dir.path());
        let broken = "---\nname: Bad\ncolor: \"\u{1b}[31m\"\n---\n";
        let incoming = [
            one("a/reviewer.md", REVIEWER),
            one("b/reviewer.md", REVIEWER),
            one("tester.md", TESTER),
            one("bad.md", broken),
            one("fine.md", "---\nname: Fine\n---\n"),
        ];

        let offered = offers(&incoming, (&listing, dir.path()));

        assert_eq!(
            problems(&offered),
            [
                Some("its id `reviewer` appears more than once in the source"),
                Some("its id `reviewer` appears more than once in the source"),
                Some("your persona `mine` is also named `Tester`"),
                Some("its name, description, emoji or color contains a control character"),
                None,
            ]
        );
        assert_eq!(offered[3].persona, None);
    }

    #[test]
    fn two_names_in_the_source_are_a_problem_on_both() {
        let dir = TempDir::new();
        let listing = crate::flows::persona::list(dir.path());

        let offered = offers(&[one("one.md", REVIEWER), one("two.md", REVIEWER)], (&listing, dir.path()));

        assert_eq!(problems(&offered), [Some("another persona in the source is also named `Code Reviewer`"); 2]);
    }
}
