use rstest::rstest;

use super::*;

mod newer {
    use super::*;

    #[rstest]
    #[case::patch("0.1.1", "0.1.0", true)]
    #[case::minor_beats_patch("0.2.0", "0.1.9", true)]
    #[case::numbers_not_text("0.10.0", "0.9.0", true)]
    #[case::with_a_v("v0.2.0", "0.1.0", true)]
    #[case::same("0.1.0", "0.1.0", false)]
    #[case::older("0.1.0", "0.2.0", false)]
    #[case::release_beats_its_prerelease("0.2.0", "0.2.0-rc.1", true)]
    #[case::prerelease_is_older("0.2.0-rc.1", "0.2.0", false)]
    #[case::garbage("latest", "0.1.0", false)]
    #[case::too_few_parts("1.2", "0.1.0", false)]
    fn compares_versions(#[case] latest: &str, #[case] current: &str, #[case] expected: bool) {
        assert_eq!(newer(latest, current), expected);
    }
}
