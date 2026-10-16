use rstest::rstest;

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|a| (*a).to_string()).collect()
}

#[rstest]
#[case::yolo(&["--yolo"], &["--force"])]
#[case::short(&["-f"], &["--force"])]
#[case::force(&["--force", "--trust"], &["--force", "--trust"])]
#[case::plan_flag(&["--plan"], &["--mode", "plan"])]
#[case::mode_equals(&["--mode=ask"], &["--mode", "ask"])]
#[case::mode_apart(&["--mode", "plan"], &["--mode", "plan"])]
#[case::other_flags(&["--model", "auto", "--sandbox", "enabled"], &["--model", "auto", "--sandbox", "enabled"])]
fn normalizes_the_spellings_of_a_mode(#[case] given: &[&str], #[case] expected: &[&str]) {
    assert_eq!(normalized(&args(given)), args(expected));
}

#[test]
fn resumes_by_chat_id() {
    assert_eq!(resume(args(&["--force"]), "abc".into()), Some(args(&["--force", "--resume", "abc"])));
}

#[rstest]
#[case::slash("/plan fix it", true)]
#[case::shell(" !ls", true)]
#[case::text("fix the bug", false)]
fn knows_local_commands(#[case] text: &str, #[case] local: bool) {
    assert_eq!(local_command(text), local);
}

#[rstest]
#[case::the_bundle("/home/a/.local/share/cursor-agent/versions/2026.10.14-941492e/index.js", true)]
#[case::another_version("/opt/cursor-agent/versions/1/index.js", true)]
#[case::another_script("/home/a/.local/share/cursor-agent/versions/2026.10.14/other.js", false)]
#[case::another_folder("/home/a/app/versions/2026.10.14/index.js", false)]
#[case::the_grok_binary("/home/a/.grok/bin/agent", false)]
#[case::a_bare_name("index.js", false)]
fn recognises_the_bundle(#[case] arg: &str, #[case] expected: bool) {
    assert_eq!(runs_as(arg), expected);
}

#[rstest]
#[case::working("Fix bug - ⏳ Working ···", Some(Activity::Working))]
#[case::planning("Cursor Agent - 🧭 Planning", Some(Activity::Working))]
#[case::shell("Cursor Agent - ⌨️ Running shell command", Some(Activity::Working))]
#[case::queued("Cursor Agent - 📋 Queued", Some(Activity::Working))]
#[case::reviewing("Cursor Agent - 📝 Reviewing changes", Some(Activity::Working))]
#[case::asking("Cursor Agent - ❓ Waiting for you", Some(Activity::Waiting))]
#[case::confirming("Cursor Agent - 🔐 Waiting for confirmation", Some(Activity::Waiting))]
#[case::ready("Cursor Agent - ✅ Ready", Some(Activity::Idle))]
#[case::in_a_worktree("Cursor Agent - ✅ Ready (feature)", Some(Activity::Idle))]
#[case::a_name_with_a_dash("Fix a - b - ⏳ Working ···", Some(Activity::Working))]
#[case::reconnecting("Cursor Agent - 🔄 Reconnecting", None)]
#[case::plain("Cursor Agent", None)]
#[case::a_chat_name("Fix a - bug", None)]
#[case::no_state("", None)]
fn reads_the_status_indicator_in_the_title(#[case] title: &str, #[case] expected: Option<Activity>) {
    assert_eq!(indicated(title), expected);
}

#[test]
fn modes_name_every_spelling_it_normalizes() {
    let table = crate::agents::modes(&crate::config::Config::default(), NAME);
    let names: Vec<&str> = table.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["plan", "ask", "auto review", "run everything (dangerous)"]);
}
