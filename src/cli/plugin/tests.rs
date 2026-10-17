use std::path::{Path, PathBuf};

use super::*;
use crate::plugins::registry::{Entry, Source};

fn evil() -> Plugin {
    let entry = Entry {
        id: "notes\u{202e}".into(),
        source: Source::Link { path: PathBuf::from("/plugins/evil\u{202e}lmth.sh\u{e0041}") },
        enabled: false,
    };
    Plugin { entry, manifest: Err("cannot read it".into()) }
}

#[test]
fn the_list_shows_invisible_characters_escaped() {
    let text = list_text(&[evil()]);
    assert!(text.contains("notes\\u{202e}  -  off") && text.contains("evil\\u{202e}lmth.sh\\u{e0041}"), "{text}");
}

#[test]
fn the_json_list_escapes_invisible_characters_and_keeps_their_values() {
    let json = list_json(&[evil()], &Places::new(Path::new("/config"), Path::new("/state")));
    assert!(!json.chars().any(|c| c != '\n' && crate::text::hidden(c)), "{json}");
    assert!(json.contains("evil\\u202elmth.sh\\udb40\\udc41"), "{json}");
    let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(read["plugins"][0]["id"], "notes\u{202e}");
}
