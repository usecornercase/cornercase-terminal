use std::sync::Arc;

use rstest::rstest;

use super::*;
use crate::changes::diff;

fn sample() -> Diff {
    let files = ["src/returns.rs", "src/order.test.js", "README.md"];
    Diff { files: files.map(|path| Arc::new(diff::untracked(path, 2, Some(b"a\n")))).to_vec() }
}

#[rstest]
#[case::a_name_anywhere("*.test.js", "src/cart/total.test.js", true)]
#[case::not_another_extension("*.test.js", "src/cart/total.js", false)]
#[case::a_folder_anywhere("vendor", "lib/vendor/x.js", true)]
#[case::a_trailing_slash_is_a_folder("dist/", "src/dist", false)]
#[case::a_slash_anchors_to_the_root("src/*.rs", "src/main.rs", true)]
#[case::one_star_stays_in_its_folder("src/*.rs", "src/ui/main.rs", false)]
#[case::two_stars_cross_folders("src/**/*.rs", "src/ui/changes/filter.rs", true)]
#[case::two_stars_match_no_folder("src/**/*.rs", "src/main.rs", true)]
#[case::a_folder_takes_what_is_inside("src/api/**", "src/api/v1/orders.ts", true)]
#[case::a_rooted_folder("/docs", "docs/readme.md", true)]
#[case::question_mark_is_one_character("?.rs", "src/a.rs", true)]
#[case::lower_case_ignores_case("*.md", "README.MD", true)]
#[case::an_upper_case_letter_matches_case("README*", "readme.md", false)]
fn globs_match_like_gitignore(#[case] pattern: &str, #[case] path: &str, #[case] expected: bool) {
    assert_eq!(glob_matches(pattern, path), expected);
}

#[rstest]
#[case::empty_keeps_everything("", [true, true, true])]
#[case::a_word_is_part_of_the_path("order", [false, true, false])]
#[case::a_word_can_hold_a_slash("src/re", [true, false, false])]
#[case::a_word_ignores_case("readme", [false, false, true])]
#[case::an_upper_case_letter_matches_case("Readme", [false, false, false])]
#[case::a_glob("*.test.js", [false, true, false])]
#[case::words_add_up("*.rs *.md", [true, false, true])]
#[case::an_exclamation_mark_leaves_out("!*.md", [true, true, false])]
#[case::includes_and_excludes("src/ !*.test.js", [true, false, false])]
#[case::a_lone_exclamation_mark_is_a_part("!", [false, false, false])]
fn keeps_the_files_whose_path_matches(#[case] query: &str, #[case] expected: [bool; 3]) {
    assert_eq!(kept(&sample(), query), expected);
}

#[test]
fn a_renamed_file_is_kept_by_its_old_path() {
    let file = File { old_path: Some("legacy/cart.js".into()), ..(*sample().files[0]).clone() };
    assert_eq!(kept(&Diff { files: vec![Arc::new(file)] }, "*.js"), [true]);
}
