use super::*;

#[test]
fn uses_the_five_hour_reset_and_rejects_missing_or_past_resets() {
    let mut report = usage::Report::default();
    assert!(reset_time(&report, 100).is_err());
    report.windows.push(usage::Window {
        label: "week".into(),
        kind: "weekly_all".into(),
        scope: None,
        percent: 100,
        severity: usage::Severity::Critical,
        resets_at: Some(200),
    });
    assert!(reset_time(&report, 100).is_err());
    report.windows.push(usage::Window {
        label: "session (5h)".into(),
        kind: "session".into(),
        scope: None,
        percent: 80,
        severity: usage::Severity::Warning,
        resets_at: Some(150),
    });
    assert_eq!(reset_time(&report, 100).expect("reset"), 150);
    assert!(reset_time(&report, 150).is_err());
}
