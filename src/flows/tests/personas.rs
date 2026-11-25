use super::*;

fn following(id: &str) -> Flow {
    let mut flow = sample();
    flow.roles[1].persona = Some(id.into());
    flow
}

#[test]
fn a_persona_must_be_an_id() {
    let problems = following("Code Reviewer").problems();
    assert_eq!(
        problems,
        ["`reviewer` follows `Code Reviewer`, which is not a persona id: use lower-case letters, digits and -"]
    );
    assert_eq!(following(" ").problems(), Vec::<String>::new());
}

#[test]
fn a_persona_not_among_yours_is_a_problem() {
    let flow = following("engineering-code-reviewer");
    assert_eq!(flow.problems(), Vec::<String>::new());
    assert_eq!(flow.persona_problems(&["engineering-code-reviewer".into()]), Vec::<String>::new());
    assert_eq!(
        flow.persona_problems(&[]),
        ["`reviewer` follows the persona `engineering-code-reviewer`, which is not among your personas or cannot be \
          read: import it with `cornercase persona import`, and `cornercase persona list` says why one cannot be read"]
    );
}

#[test]
fn a_file_names_the_persona_only_when_a_role_has_one() {
    let text = store::to_text(&following("engineering-code-reviewer")).expect("serialize");
    assert!(text.contains("persona = \"engineering-code-reviewer\""), "{text}");
    assert!(text.starts_with("version = 1\n"), "{text}");
    assert_eq!(store::parse(&text).expect("parse"), following("engineering-code-reviewer"));
    assert!(!store::to_text(&sample()).expect("serialize").contains("persona"));
}
