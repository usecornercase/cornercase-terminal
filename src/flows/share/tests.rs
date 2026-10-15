use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::flows::store::{Listing, Saved};
use crate::flows::{builtin, store};

fn sample() -> Flow {
    builtin::flows(&Known::ALL, Some(Known::Claude)).remove(0)
}

fn with_env() -> Flow {
    let mut flow = sample();
    flow.roles[0].env.insert("AWS_PROFILE".into(), "work".into());
    flow.roles[0].args = vec!["--verbose".into()];
    flow
}

mod the_text {
    use super::*;

    #[test]
    fn every_built_in_flow_survives_export_then_import() {
        for flow in builtin::flows(&Known::ALL, None) {
            let text = to_share(&flow).expect("export");
            assert!(text.starts_with("# A cornercase flow"), "{text}");
            assert_eq!(from_share(&text).expect("import"), flow);
        }
    }

    #[test]
    fn the_header_is_only_a_comment() {
        let flow = sample();
        let plain = store::to_text(&flow).expect("serialize");
        assert_eq!(from_share(&plain).expect("import"), flow);
    }

    #[test]
    fn a_byte_order_mark_and_crlf_line_endings_are_accepted() {
        let flow = sample();
        let text = to_share(&flow).expect("export").replace('\n', "\r\n");
        assert_eq!(from_share(&format!("\u{feff}{text}")).expect("import"), flow);
    }

    #[test]
    fn exactly_the_cap_is_accepted_and_one_byte_more_is_refused() {
        let flow = sample();
        let mut text = to_share(&flow).expect("export");
        let _ = write!(text, "\n#{}", "x".repeat(MAX_BYTES - text.len() - 2));
        assert_eq!(text.len(), MAX_BYTES);
        assert!(from_share(&text).is_ok());
        text.push('x');
        assert_eq!(from_share(&text), Err(too_large()));
    }

    #[test]
    fn a_flow_too_large_to_share_is_refused_on_export() {
        let mut flow = sample();
        flow.notes = "x".repeat(MAX_BYTES);
        assert_eq!(to_share(&flow), Err(too_large()));
    }

    #[test]
    fn a_newer_version_is_refused() {
        let error = from_share("version = 99\nname = \"x\"").expect_err("refused");
        assert!(error.contains("newer cornercase"), "{error}");
    }

    #[test]
    fn bad_toml_names_its_line() {
        let error = from_share("name = \"x\"\n\nend = = 3\n").expect_err("refused");
        assert!(error.starts_with("line 3: "), "{error}");
    }

    #[test]
    fn control_characters_in_names_labels_and_variable_names_are_problems() {
        let bad = "x\x1b[2J";
        let mut flows = vec![sample(); 5];
        flows[0].name = format!("Bad{bad}");
        flows[1].description = bad.into();
        flows[2].roles[0].name = bad.into();
        flows[3].roles[0].agent = bad.into();
        flows[4].roles[0].env.insert(bad.into(), "v".into());
        for flow in &flows {
            let read = from_share(&to_share(flow).expect("export")).expect("it parses");
            assert!(read.problems().iter().any(|p| p.contains("control character")), "{:?}", read.problems());
        }
        assert!(sample().problems().iter().all(|p| !p.contains("control character")));
    }

    #[test]
    fn the_orchestrators_name_is_forced() {
        let text = "name = \"x\"\n[orchestrator]\nname = \"boss\"\nagent = \"claude\"\n";
        assert_eq!(from_share(text).expect("import").orchestrator.name, ORCHESTRATOR);
    }
}

mod the_risks {
    use super::*;

    #[test]
    fn they_name_the_variables_and_the_arguments_of_each_role() {
        let flow = with_env();
        let role = flow.roles[0].name.clone();
        let risks = risks(&flow, "this imported flow").expect("risks");
        assert!(risks.starts_with("this imported flow sets AWS_PROFILE=work for "), "{risks}");
        let long = Flow {
            roles: vec![Role { env: [("K".into(), "v".repeat(60))].into(), ..flow.roles[0].clone() }],
            ..flow.clone()
        };
        assert!(super::risks(&long, "it").expect("risks").contains(&format!("K={}…", "v".repeat(39))), "{risks}");
        assert!(key_risks(&flow, "it").expect("risks").starts_with("it sets AWS_PROFILE for "));
        assert!(risks.contains(&format!("`{role}` and passes --verbose to `{role}`")), "{risks}");
        assert!(risks.ends_with("they run without asking, so check them"), "{risks}");
    }

    #[test]
    fn control_characters_in_values_arguments_and_names_are_shown_not_sent() {
        let mut flow = with_env();
        flow.roles[0].env.insert("K".into(), "x\r\x1b[2Jy".into());
        flow.roles[0].args = vec!["a\nb\x1b[1A".into()];
        flow.name = "n\x1b[2Jm".into();
        let shown = format!("{}{}", risks(&flow, "it").expect("risks"), summary(&flow));
        assert!(shown.contains("K=x\\r\\u{1b}[2Jy") && shown.contains("a\\nb\\u{1b}[1A"), "{shown}");
        assert!(!shown.chars().any(|c| c.is_control() && c != '\n'), "{shown:?}");
    }

    #[test]
    fn a_flow_without_variables_or_arguments_has_none() {
        assert_eq!(risks(&sample(), "it"), None);
        assert_eq!(env_warning(&sample()), None);
    }

    #[test]
    fn the_export_warning_names_the_variables() {
        let warning = env_warning(&with_env()).expect("warning");
        assert!(warning.contains("AWS_PROFILE") && warning.contains("no secret"), "{warning}");
    }

    #[test]
    fn missing_agents_name_the_roles_that_cannot_start() {
        let flow = sample();
        assert_eq!(missing_agents(&flow, &Known::ALL), Vec::<String>::new());
        let missing = missing_agents(&flow, &[]);
        assert_eq!(missing.len(), flow.agents(), "{missing:?}");
        assert!(missing[0].contains("not installed here"), "{missing:?}");
    }

    #[test]
    fn the_summary_lists_each_role_with_its_agent() {
        let summary = summary(&sample());
        assert!(summary.lines().count() > 2 && summary.contains("orchestrator: claude"), "{summary}");
    }
}

mod collisions {
    use super::*;

    fn listing() -> Listing {
        let mut flows: Vec<Saved> = vec![
            Saved { flow: named("Quick"), origin: Origin::BuiltIn },
            Saved { flow: named("Mine"), origin: Origin::User(PathBuf::from("/u/mine.toml")) },
            Saved { flow: named("Theirs"), origin: Origin::Project(PathBuf::from("/p/theirs.toml")) },
        ];
        flows.sort_by_key(|saved| saved.flow.name.clone());
        Listing { flows, broken: Vec::new(), builtins: vec!["Quick".into()] }
    }

    fn named(name: &str) -> Flow {
        Flow { name: name.into(), ..sample() }
    }

    #[rstest]
    #[case::yours_over_yours(To::Yours, "mine", false, Err(()))]
    #[case::yours_over_yours_replaced(To::Yours, "Mine ", true, Ok(Some("/u/mine.toml")))]
    #[case::yours_over_a_built_in(To::Yours, "QUICK", false, Err(()))]
    #[case::yours_over_a_built_in_replaced(To::Yours, "Quick", true, Ok(None))]
    #[case::yours_over_the_projects(To::Yours, "Theirs", false, Ok(None))]
    #[case::yours_new(To::Yours, "Fresh", false, Ok(None))]
    #[case::project_over_yours(To::Project, "Mine", false, Err(()))]
    #[case::project_over_yours_even_replaced(To::Project, "Mine", true, Err(()))]
    #[case::project_over_a_built_in(To::Project, "Quick", true, Err(()))]
    #[case::project_over_the_projects(To::Project, "theirs", false, Err(()))]
    #[case::project_over_the_projects_replaced(To::Project, "theirs", true, Ok(Some("/p/theirs.toml")))]
    #[case::project_new(To::Project, "Fresh", false, Ok(None))]
    fn the_table(
        #[case] to: To,
        #[case] name: &str,
        #[case] replace: bool,
        #[case] expected: Result<Option<&str>, ()>,
    ) {
        let got = collision(&listing(), name, to, replace);
        if let Ok(path) = expected {
            assert_eq!(got, Ok(path.map(PathBuf::from)));
        } else {
            let error = got.expect_err("refused");
            assert!(error.contains("--name"), "{error}");
        }
    }
}
