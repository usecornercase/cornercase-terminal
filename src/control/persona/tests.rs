use serde_json::json;

use super::*;
use crate::control::{Command, RoleInfo, RoleSpec};
use crate::flows::Role;

#[test]
fn an_import_reads_with_every_field_left_out() {
    let import: PersonaImport =
        serde_json::from_value(json!({"personas": [{"text": "---\nname: X\n---\n"}]})).expect("an import");

    assert_eq!(import.personas[0].file, "");
    assert_eq!(import.personas[0].source, None);
    assert!(!import.replace && !import.check);
}

#[test]
fn an_imports_logged_fields_carry_its_count_and_size_and_never_its_text() {
    let text = PersonaText { file: "secret.md".into(), text: "---\nname: SECRET\n---\n".into(), source: None };
    let fields = Command::PersonaImport(PersonaImport { personas: vec![text], ..PersonaImport::default() }).fields();

    assert!(fields.contains(&("count", "1".to_string())), "{fields:?}");
    assert!(fields.contains(&("text_bytes", "21".to_string())), "{fields:?}");
    assert!(!format!("{fields:?}").contains("SECRET"), "{fields:?}");
}

#[test]
fn a_role_without_a_persona_reads_as_before() {
    let role = Role { name: "coder".into(), agent: "claude".into(), ..Role::default() };

    let spec = serde_json::to_value(RoleSpec::from(&role)).expect("json");
    let info = serde_json::to_value(RoleInfo::of_flow(&crate::flows::Flow::default())).expect("json");

    assert!(spec.get("persona").is_none(), "{spec}");
    assert!(info[0].get("persona").is_none(), "{info}");
}

#[test]
fn a_roles_persona_goes_to_clients_and_back() {
    let role = Role { name: "reviewer".into(), persona: Some("engineering-code-reviewer".into()), ..Role::default() };
    let flow = crate::flows::Flow { roles: vec![role], ..crate::flows::Flow::default() };

    let spec = serde_json::to_value(crate::control::FlowSpec::from(&flow)).expect("json");
    let back: crate::control::FlowSpec = serde_json::from_value(spec.clone()).expect("a spec");

    assert_eq!(spec["roles"][0]["persona"], "engineering-code-reviewer");
    assert_eq!(back.into_flow().roles[0].persona.as_deref(), Some("engineering-code-reviewer"));
}

#[test]
fn a_listed_persona_leaves_out_what_it_lacks() {
    let info = PersonaInfo { id: "r".into(), name: "R".into(), bytes: 3, ..PersonaInfo::default() };

    assert_eq!(
        serde_json::to_value(info).expect("json"),
        json!({"id": "r", "name": "R", "description": "", "bytes": 3})
    );
}
