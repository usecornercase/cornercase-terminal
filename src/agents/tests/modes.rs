use super::*;

#[test]
fn claude_has_the_default_modes() {
    let names: Vec<String> = modes(&config(), "claude").into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["accept edits", "auto", "plan", "skip permissions (dangerous)"]);
}

#[rstest]
#[case::short(&["gemini", "-y"], "yolo (dangerous)")]
#[case::long(&["gemini", "--yolo"], "yolo (dangerous)")]
#[case::approval(&["gemini", "--approval-mode", "yolo"], "yolo (dangerous)")]
#[case::equals(&["gemini", "--approval-mode=yolo"], "yolo (dangerous)")]
#[case::plan(&["gemini", "--approval-mode", "plan"], "plan")]
fn gemini_modes_include_plan_and_recognize_yolo_aliases(#[case] args: &[&str], #[case] expected: &str) {
    let modes = modes(&config(), Known::Gemini.name());
    assert_eq!(mode_of(Known::Gemini.name(), &strings(args), &modes).as_deref(), Some(expected));
    assert_eq!(
        with_mode(Known::Gemini.name(), &strings(&args[1..]), &modes, Some("plan")),
        strings(&["--approval-mode", "plan"])
    );
}

#[test]
fn opencode_plans_with_its_plan_agent_and_approves_with_auto() {
    let kind = Known::Opencode.name();
    let modes = modes(&config(), kind);
    let names: Vec<&str> = modes.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["plan", "auto (dangerous)"]);
    assert_eq!(mode_of(kind, &strings(&["opencode", "--agent", "plan"]), &modes).as_deref(), Some("plan"));
    assert_eq!(with_mode(kind, &strings(&["--auto", "-c"]), &modes, Some("plan")), strings(&["--agent", "plan", "-c"]));
}

#[test]
fn grok_modes_follow_its_permission_flags() {
    let modes = modes(&config(), Known::Grok.name());
    let kind = Known::Grok.name();
    assert_eq!(mode_of(kind, &strings(&["grok", "--permission-mode", "auto"]), &modes).as_deref(), Some("auto"));
    assert_eq!(mode_of(kind, &strings(&["grok", "--yolo"]), &modes).as_deref(), Some("always approve (dangerous)"));
    assert_eq!(mode_of(kind, &strings(&["grok", "--permission-mode=dontAsk"]), &modes), None);
}

#[rstest]
#[case::qwen("qwen")]
#[case::custom("mine")]
fn another_agents_yolo_mode_keeps_its_own_arguments(#[case] kind: &str) {
    let mut config = config();
    config.agent_modes.insert(kind.into(), [("yolo".into(), strings(&["--yolo"]))].into());
    let modes = modes(&config, kind);
    let args = strings(&["-y", "--approval-mode=custom"]);
    assert_eq!(mode_of(kind, &args, &modes), None);
    assert_eq!(extra_args(kind, &args, &modes), args);
    assert_eq!(with_mode(kind, &args, &modes, None), args);
    assert_eq!(with_mode(kind, &args, &modes, Some("yolo")), strings(&["--yolo", "-y", "--approval-mode=custom"]));
}

#[test]
fn the_config_adds_replaces_and_hides_modes() {
    let mut c = config();
    let mine = BTreeMap::from([
        ("plan".to_string(), strings(&["--plan"])),
        ("auto".to_string(), Vec::new()),
        ("quiet".to_string(), strings(&["-q"])),
    ]);
    c.agent_modes.insert("claude".into(), mine);
    let found = modes(&c, "claude");
    assert_eq!(
        found.iter().map(|(n, a)| (n.as_str(), a.join(" "))).collect::<Vec<_>>(),
        [
            ("accept edits", "--permission-mode acceptEdits".to_string()),
            ("plan", "--plan".into()),
            ("skip permissions (dangerous)", "--dangerously-skip-permissions".into()),
            ("quiet", "-q".into())
        ]
    );
}

#[test]
fn a_mode_is_found_in_the_arguments() {
    let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
    assert_eq!(mode_of(Known::Claude.name(), &args, &modes(&config(), "claude")).as_deref(), Some("plan"));
}

#[test]
fn extra_arguments_are_the_rest() {
    let args = strings(&["--add-dir", "x", "--permission-mode", "plan"]);
    assert_eq!(extra_args(Known::Claude.name(), &args, &modes(&config(), "claude")), strings(&["--add-dir", "x"]));
}

#[test]
fn picking_a_mode_replaces_the_old_one_and_keeps_the_extras() {
    let args = strings(&["--permission-mode", "plan", "--add-dir", "x"]);
    let next =
        with_mode(Known::Claude.name(), &args, &modes(&config(), "claude"), Some("skip permissions (dangerous)"));
    assert_eq!(next, strings(&["--dangerously-skip-permissions", "--add-dir", "x"]));
}

#[test]
fn no_mode_keeps_only_the_extras() {
    let args = strings(&["--permission-mode", "plan", "-v"]);
    assert_eq!(with_mode(Known::Claude.name(), &args, &modes(&config(), "claude"), None), strings(&["-v"]));
}

#[test]
fn new_extras_keep_the_mode() {
    let args = strings(&["--sandbox", "read-only", "-v"]);
    let next = with_extra(Known::Codex.name(), &args, &modes(&config(), "codex"), strings(&["--model", "o3"]));
    assert_eq!(next, strings(&["--sandbox", "read-only", "--model", "o3"]));
}

#[rstest]
#[case::dangerous("skip permissions (dangerous)", true)]
#[case::safe("plan", false)]
fn dangerous_modes_say_so_in_their_name(#[case] name: &str, #[case] expected: bool) {
    assert_eq!(is_dangerous(name), expected);
}

#[test]
fn a_mode_named_default_in_the_config_is_ignored() {
    let mut c = config();
    c.agent_modes.insert(
        "claude".into(),
        [("default".to_string(), strings(&["--foo"])), ("quiet".to_string(), strings(&["-q"]))].into(),
    );
    let names: Vec<String> = modes(&c, "claude").into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["accept edits", "auto", "plan", "skip permissions (dangerous)", "quiet"]);
    c.agent_args.insert("claude".into(), strings(&["--foo"]));
    assert_eq!(start_line(&c, "claude", Some("default"), None, None).as_deref(), Ok("claude --foo"));
}
