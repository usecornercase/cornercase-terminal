use super::*;
use crate::control::persona::CHECK;

fn found(path: &str, division: Option<&str>) -> Found {
    let file = path.rsplit('/').next().unwrap_or_default().to_string();
    Found { file, path: path.into(), text: String::new(), source: None, division: division.map(str::to_string) }
}

fn gathered() -> Gathered {
    Gathered {
        found: vec![found("engineering/reviewer.md", Some("engineering")), found("loose.md", None)],
        skipped: 3,
        divisions: vec![Division {
            key: "engineering".into(),
            label: "Engineering".into(),
            icon: Some("code".into()),
            color: Some("#3B82F6".into()),
        }],
    }
}

fn answered() -> PersonasImported {
    let reviewer = PersonaInfo {
        id: "reviewer".into(),
        name: "Code Reviewer".into(),
        emoji: "👁️".into(),
        bytes: 2_000,
        replaced: true,
        ..PersonaInfo::default()
    };
    let loose =
        PersonaInfo { bytes: 10, problem: Some("its front matter gives no `name`".into()), ..PersonaInfo::default() };
    PersonasImported { personas: vec![reviewer, loose], check: CHECK.into(), saved: false }
}

fn octo() -> Source {
    Source::Repo { repo: "octo/agents".into(), path: String::new() }
}

#[test]
fn the_json_names_each_persona_its_file_division_and_problem() {
    let shown = serde_json::to_value(catalogue(&octo(), gathered(), answered())).expect("json");

    assert_eq!(
        shown,
        serde_json::json!({
            "label": "octo/agents",
            "page": "https://github.com/octo/agents",
            "web": true,
            "check": format!("from octo/agents (see https://github.com/octo/agents), which nobody here reviewed: {CHECK}"),
            "skipped": 3,
            "divisions": [{"key": "engineering", "label": "Engineering", "icon": "code", "color": "#3B82F6"}],
            "personas": [
                {"file": "engineering/reviewer.md", "division": "engineering", "id": "reviewer",
                 "name": "Code Reviewer", "description": "", "emoji": "👁️", "color": "", "bytes": 2000,
                 "replaced": true, "problem": null},
                {"file": "loose.md", "division": null, "id": "", "name": "", "description": "", "emoji": "",
                 "color": "", "bytes": 10, "replaced": false, "problem": "its front matter gives no `name`"}
            ]
        })
    );
}

#[test]
fn the_list_groups_by_division_and_says_why_one_cannot_be_imported() {
    let shown = readable(&catalogue(&octo(), gathered(), answered()));

    assert_eq!(
        shown,
        format!(
            "Engineering (1 persona)\n  reviewer: 👁️ Code Reviewer · 2 KiB, replaces yours\npersonas (1 persona)\n  \
             loose.md: cannot be imported: its front matter gives no `name`\nfrom octo/agents (see \
             https://github.com/octo/agents), which nobody here reviewed: {CHECK}\nskipped 3 files that are not \
             personas\n2 personas in octo/agents, 1 of them cannot be imported; nothing was imported\n"
        )
    );
}
