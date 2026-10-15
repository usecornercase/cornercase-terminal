use super::*;

fn hit(pane: u64, text: &str) -> Hit {
    Hit {
        pane,
        tab: 3,
        workspace: 2,
        project: 1,
        place: "shop › main › claude".into(),
        who: "claude".into(),
        source: "reply".into(),
        text: text.into(),
        ..Hit::default()
    }
}

#[test]
fn no_match_prints_the_header_alone() {
    assert_eq!(table(&Matches::default()).expect("a table"), "pane\twho\tsource\tplace\tline\ttext\n");
}

#[test]
fn a_row_has_a_dash_for_what_is_missing() {
    let found = Matches { matches: vec![hit(5, "fixed the checkout")], capped: false };

    assert_eq!(
        table(&found).expect("a table"),
        "pane\twho\tsource\tplace\tline\ttext\n5\tclaude\treply\tshop › main › claude\t-\tfixed the checkout\n"
    );
}

#[test]
fn control_characters_and_tabs_become_spaces_so_a_row_stays_one_line() {
    let found = Matches { matches: vec![hit(5, "a\tb\u{1b}[0m\nc")], capped: false };

    let text = table(&found).expect("a table");

    assert_eq!(text.lines().count(), 2);
    assert_eq!(text.lines().nth(1).map(|row| row.split('\t').count()), Some(6));
    assert!(text.contains("a b [0m c"), "{text:?}");
}

#[test]
fn the_columns_are_the_keys_of_a_hit_in_order() {
    let value = serde_json::to_value(Hit::default()).expect("json");

    assert!(Hit::COLUMNS.iter().all(|column| value.get(*column).is_some()));
}
