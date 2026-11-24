use std::path::PathBuf;

use super::*;
use crate::control::persona::BrokenPersona;

fn reviewer() -> PersonaInfo {
    PersonaInfo {
        id: "engineering-code-reviewer".into(),
        name: "Code Reviewer".into(),
        description: "Reviews code.".into(),
        emoji: "👁️".into(),
        source: Some("msitarzewski/agency-agents/engineering/engineering-code-reviewer.md".into()),
        bytes: 4_100,
        ..PersonaInfo::default()
    }
}

#[test]
fn the_list_names_each_persona_its_source_and_what_cannot_be_read() {
    let personas = Personas {
        dir: PathBuf::from("/c/personas"),
        personas: vec![reviewer()],
        broken: vec![BrokenPersona {
            path: PathBuf::from("/c/personas/x.md"),
            error: "it has no\nfront matter".into(),
        }],
    };

    assert_eq!(
        listing(&personas),
        "engineering-code-reviewer: 👁️ Code Reviewer · 5 KiB\n  Reviews code.\n  from \
         msitarzewski/agency-agents/engineering/engineering-code-reviewer.md\ncannot read /c/personas/x.md: it has \
         no\\nfront matter\n"
    );
}

#[test]
fn an_empty_list_says_how_to_import() {
    let personas = Personas { dir: PathBuf::from("/c/personas"), ..Personas::default() };
    assert!(listing(&personas).starts_with("no personas in /c/personas yet: `cornercase persona import"));
}

#[test]
fn a_preview_lists_what_would_come_and_saves_nothing() {
    let preview = PersonasImported {
        personas: vec![PersonaInfo { replaced: true, ..reviewer() }],
        check: CHECK.into(),
        saved: false,
    };

    let text = checked(&preview);

    assert!(text.starts_with("engineering-code-reviewer: 👁️ Code Reviewer · 5 KiB, replaces yours\n"), "{text}");
    assert!(text.ends_with(&format!("{CHECK}\nwould import 1 persona; nothing was imported\n")), "{text}");
}

#[test]
fn a_source_from_the_web_is_named_with_where_to_read_it() {
    let source = Source::Repo { repo: "octo/agents".into(), path: "engineering".into() };

    assert_eq!(
        warning(&source, CHECK),
        format!(
            "from octo/agents/engineering (see https://github.com/octo/agents/tree/HEAD/engineering), which nobody here reviewed: {CHECK}"
        )
    );
    assert_eq!(warning(&Source::File("r.md".into()), CHECK), CHECK);
}

#[test]
fn an_import_names_each_persona_and_its_file() {
    let answer = PersonasImported {
        personas: vec![PersonaInfo { path: Some(PathBuf::from("/c/personas/r.md")), ..reviewer() }],
        saved: true,
        ..PersonasImported::default()
    };

    assert_eq!(imported(&answer), "imported Code Reviewer as engineering-code-reviewer: /c/personas/r.md\n");
}

mod only {
    use super::*;

    const REVIEWER: &str = "---\nname: Code Reviewer\n---\nReview it.\n";
    const TESTER: &str = "---\nname: Tester\n---\nTest it.\n";

    fn found(file: &str, text: &str) -> Found {
        Found { file: file.into(), path: file.into(), text: text.into(), source: None, division: None }
    }

    fn all() -> Vec<Found> {
        vec![found("reviewer.md", REVIEWER), found("tester.md", TESTER), found("broken.md", "---\nname:\n---\n")]
    }

    fn octo() -> Source {
        Source::Repo { repo: "octo/agents".into(), path: String::new() }
    }

    #[test]
    fn keeps_only_the_ids_asked_for() {
        let kept = only(all(), &["tester".into()], &octo()).expect("kept");
        assert_eq!(kept.iter().map(|found| found.file.as_str()).collect::<Vec<_>>(), ["tester.md"]);
    }

    #[test]
    fn without_ids_keeps_everything() {
        assert_eq!(only(all(), &[], &octo()).expect("kept").len(), 3);
    }

    #[test]
    fn an_id_not_in_the_source_is_an_error_naming_it() {
        let Err(Error::Control(e)) = only(all(), &["tester".into(), "nobody".into(), "nul\u{1b}".into()], &octo())
        else {
            panic!("refused")
        };
        assert_eq!(
            e,
            "octo/agents has no personas `nobody`, `nul\\u{1b}`: `persona import octo/agents --catalogue` lists its ids"
        );
    }
}
