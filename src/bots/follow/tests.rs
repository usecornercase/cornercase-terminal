use super::*;

#[test]
fn the_texts_are_fixed() {
    assert_eq!(started("claude", "web › fix"), "▶ started claude in web › fix");
    assert_eq!(finished("claude", "web", false), "✓ claude finished in web");
    assert_eq!(finished("claude", "web", true), "✗ claude failed in web");
    assert_eq!(waiting("claude", "web"), "! claude needs you in web");
    assert_eq!(dropped("claude", "web"), "no longer following claude in web: a bot follows at most 20 agents");
}

#[test]
fn a_body_is_cut_at_2_kb_on_a_character_boundary_and_says_so() {
    assert_eq!(body("  short  "), "short");
    let long = "é".repeat(BODY_CAP);
    let cut = body(&long);
    assert!(cut.ends_with("\n… (cut)"), "{cut}");
    assert!(cut.len() <= BODY_CAP + "\n… (cut)".len());
    assert!(cut.starts_with('é'));
}

#[test]
fn a_report_falls_back_to_the_screen() {
    assert_eq!(
        report("web", 3, false, Said::Message("done")),
        "The agent you started in web (pane 3) finished. Its last message:\n\ndone"
    );
    assert_eq!(
        report("web", 3, true, Said::Screen("boom")),
        "The agent you started in web (pane 3) failed. Its screen ends with:\n\nboom"
    );
}

#[test]
fn a_batch_keeps_at_least_one_and_stops_at_16_kib() {
    let big = "x".repeat(REPORTS_CAP + 1);
    assert_eq!(batch(&[big.clone(), String::from("a")]), 1);
    assert_eq!(batch(&[String::from("a"), String::from("b")]), 2);
    let half = "x".repeat(REPORTS_CAP / 2 + 1);
    assert_eq!(batch(&[half.clone(), half.clone(), half]), 1);
    assert_eq!(batch(&[]), 0);
}

#[test]
fn an_orchestrator_is_over_at_the_runs_end_or_when_the_run_stands_still() {
    let standing = |ended: bool, still: Option<u64>| Standing { ended, still: still.map(Duration::from_secs) };
    assert!(!orchestrator_over(true, standing(false, None)));
    assert!(orchestrator_over(true, standing(true, None)));
    assert!(!orchestrator_over(false, standing(true, None)));
    assert!(!orchestrator_over(false, standing(false, Some(59))));
    assert!(orchestrator_over(false, standing(false, Some(60))));
}

#[test]
fn a_failed_line_is_told_from_its_text() {
    assert!(failed(&finished("claude", "web", true)));
    assert!(!failed(&finished("claude", "web", false)));
}
