use rstest::rstest;

use super::*;

fn bot(id: &str, name: &str) -> Bot {
    sample(id, name)
}

mod names {
    use super::*;

    #[rstest]
    #[case::empty("")]
    #[case::forty_one(&"a".repeat(41))]
    #[case::control("a\u{7}b")]
    #[case::newline("a\nb")]
    #[case::duplicate_in_another_case("PRS")]
    fn are_refused(#[case] name: &str) {
        let taken = bot("prs", "prs");
        assert!(name_refusal(name, &[&taken]).is_some(), "{name:?}");
    }

    #[rstest]
    #[case::forty(&"a".repeat(40))]
    #[case::forty_accents(&"é".repeat(40))]
    #[case::spaces("Review my PRs")]
    fn are_taken(#[case] name: &str) {
        assert_eq!(name_refusal(name, &[]), None);
    }
}

mod ids {
    use super::*;

    #[rstest]
    #[case::words("PRs review", "prs-review")]
    #[case::nothing_to_keep("!!!", "bot")]
    fn are_a_slug_of_the_name(#[case] name: &str, #[case] id: &str) {
        assert_eq!(new_id(name, &[]), id);
    }

    #[test]
    fn a_taken_one_gets_a_number() {
        let (first, second) = (bot("prs", "PRs"), bot("prs-2", "Other"));
        assert_eq!(new_id("prs", &[&first]), "prs-2");
        assert_eq!(new_id("prs", &[&first, &second]), "prs-3");
    }
}

#[test]
fn a_bot_is_found_by_its_id_and_by_its_name_in_any_case() {
    let bot = bot("prs-review", "PRs Review");
    let all = [&bot];
    assert!(find(&all, "prs-review").is_some());
    assert!(find(&all, "prs review").is_some());
    assert!(find(&all, "other").is_none());
}

#[test]
fn there_can_be_fifty_bots() {
    assert_eq!(MAX_BOTS, 50);
}
