mod input_box {
    use rstest::rstest;

    use super::super::input_box;
    use crate::agents::{CLAUDE, CODEX};

    const RULE: &str = "────────────────────────────────────────";
    const FOOTER: &str = "  ⏵⏵ auto mode on · 1 shell · ← for agents";

    fn screen(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[rstest]
    #[case::empty(&["● Done.", "", RULE, "❯\u{a0}", RULE, FOOTER], Some(true))]
    #[case::with_text(&[RULE, "❯\u{a0}add tests", "  and run them", RULE, FOOTER], Some(true))]
    #[case::a_suggestion_list_above_it(&["  ❯ /model   Set the AI model", RULE, "❯\u{a0}/model", RULE], Some(true))]
    #[case::the_shell_details_panel(
        &["⎿  Command was manually backgrounded", RULE, "  Shell details", "  Status:   running",
          "  ← to go back · Esc/Enter/Space to close · x to stop"],
        Some(false)
    )]
    #[case::the_model_picker(&[RULE, "   Select model", "   ❯ 2.  Opus 5.5 ✔", "     3.  Fable 5.1"], Some(false))]
    #[case::the_transcript_view(&[RULE, "  Showing detailed transcript · ctrl+o to toggle"], Some(false))]
    #[case::the_prompt_history(&["  ❯ 49s ago  !sleep 300", "  ╭──────╮", "  │ ⌕ Filter history… │"], Some(false))]
    #[case::a_past_prompt_in_the_transcript(&["", "❯ fix the login", "", "● Fixed."], Some(false))]
    #[case::its_shell_mode(&[RULE, "! sleep 300", RULE, "  ! for shell mode"], Some(false))]
    fn claudes_box_is_its_marker_right_under_a_rule(#[case] lines: &[&str], #[case] expected: Option<bool>) {
        assert_eq!(input_box(CLAUDE, &screen(lines)), expected);
    }

    #[test]
    fn other_agents_are_not_known() {
        assert_eq!(input_box(CODEX, &screen(&["› Ask Codex to do anything"])), None);
    }
}

mod survey {
    use rstest::rstest;

    use super::super::survey;
    use crate::agents::{CLAUDE, CODEX};

    const RULE: &str = "────────────────────────────────────────";
    const QUESTION: &str = "● How is Claude doing this session? (optional)";
    const OPTIONS: &str = "  1: Bad    2: Fine   3: Good   0: Dismiss";
    const EFFORT: &str = "                                                  ◐ medium · /effort";
    const FOOTER: &str = "  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents";

    fn screen(lines: &[&str]) -> String {
        [lines, &[RULE, "❯\u{a0}", RULE, FOOTER]].concat().join("\n")
    }

    #[rstest]
    #[case::with_the_effort_hint_under_it(&["● Done.", "", QUESTION, OPTIONS, EFFORT])]
    #[case::with_a_blank_line_under_it(&[QUESTION, OPTIONS, ""])]
    #[case::its_question_wrapped(&["● How is Claude doing this session?", "  (optional)", OPTIONS, ""])]
    #[case::the_long_context_question(
        &["● How well is Claude following the instructions you gave earlier in this", "  conversation? (optional)",
          OPTIONS, ""]
    )]
    #[case::a_plugins_survey_with_unsure(
        &["● How helpful has the github plugin been? (optional)", "  You've used its skills recently",
          "  1: Bad    2: Fine   3: Good   4: Unsure 0: Dismiss", ""]
    )]
    fn is_its_question_and_answers_right_above_the_input_box(#[case] lines: &[&str]) {
        assert!(survey(CLAUDE, &screen(lines)));
    }

    #[rstest]
    #[case::the_input_box_alone(&["● Done.", ""])]
    #[case::the_words_in_the_conversation_above(
        &["❯ what does the survey look like", "",
          "● Claude Code shows a survey, How is Claude doing this session? (optional) with 1: Bad  2: Fine  3:",
          "  Good  0: Dismiss, above its input box. It looks like this:", "",
          "  ● How is Claude doing this session? (optional)", "    1: Bad    2: Fine   3: Good   0: Dismiss", "",
          "✻ Baked for 0s · done 6:18 PM", "", ""]
    )]
    #[case::a_quote_right_above_the_input_box(
        &["● It looks like this:", "", "  ● How is Claude doing this session? (optional)",
          "    1: Bad    2: Fine   3: Good   0: Dismiss", ""]
    )]
    #[case::the_issues_words_ending_a_message(
        &["● Here is how it reads:", "  How is Claude doing this session? (optional)",
          "  1: Bad  2: Fine  3: Good  0: Dismiss", ""]
    )]
    #[case::a_copy_ending_a_message_and_its_turn(
        &["● Here is how it reads:", "  How is Claude doing this session? (optional)", OPTIONS, "",
          "✻ Crunched for 0s · done 6:22 PM", ""]
    )]
    #[case::its_lines_higher_up(&[QUESTION, OPTIONS, "", "● Done.", "", "✻ Baked for 0s · done 6:18 PM", ""])]
    #[case::answers_without_their_question(&["● Done.", "", OPTIONS, ""])]
    #[case::answers_cut_in_a_very_narrow_pane(
        &["● How is Claude doing this session?", "  (optional)", "  1: Bad    2: Fine  3: Good   0:",
          "                               Dismiss", "                    ◐ medium · /effort"]
    )]
    fn is_nothing_else(#[case] lines: &[&str]) {
        assert!(!survey(CLAUDE, &screen(lines)));
    }

    #[test]
    fn needs_the_input_box_under_it() {
        let panel = [QUESTION, OPTIONS, "", RULE, "  Shell details", "  x to stop"].join("\n");
        assert!(!survey(CLAUDE, &panel));
    }

    #[test]
    fn is_only_claudes() {
        assert!(!survey(CODEX, &screen(&[QUESTION, OPTIONS, ""])));
    }
}
