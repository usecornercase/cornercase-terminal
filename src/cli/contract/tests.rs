use super::*;
use crate::control::contract::{self, Operation, Stream};

fn descriptor() -> Descriptor {
    Descriptor {
        version: "0.12.104".into(),
        contract: 1,
        boot: "4242-1700000000".into(),
        operations: vec![
            Operation { name: "status".into(), available: true, reason: None },
            Operation { name: "search".into(), available: false, reason: Some("turned off".into()) },
            Operation { name: "send".into(), available: true, reason: None },
        ],
        features: vec!["boot".into(), "structured-errors".into()],
        streams: vec![Stream { name: "events".into(), version: 1 }],
    }
}

#[test]
fn the_summary_names_the_version_contract_boot_and_streams() {
    assert_eq!(
        summary(&descriptor()),
        "cornercase 0.12.104\ncontract 1\nboot 4242-1700000000\ncommands 3 (2 available)\n  search unavailable: \
         turned off\nstream events 1\nfeatures boot, structured-errors\n"
    );
}

#[test]
fn every_command_of_this_build_is_counted() {
    let text = summary(&contract::describe("1-2", &[]));

    let count = control::Command::NAMES.len();
    assert!(text.contains(&format!("commands {count} ({count} available)\n")), "{text}");
    assert!(!text.contains("unavailable"), "{text}");
}

#[test]
fn an_answer_with_fields_it_does_not_know_still_reads() {
    let value = serde_json::json!({"version": "9.0.0", "contract": 2, "boot": "b", "later": true});

    let read: Descriptor = answer(value).expect("a descriptor");

    assert_eq!(summary(&read), "cornercase 9.0.0\ncontract 2\nboot b\ncommands 0 (0 available)\n");
}
