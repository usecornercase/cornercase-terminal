use super::*;

const ROW: Rect = Rect { x: 4, y: 7, width: 60, height: 1 };

fn pill(agent: &str, account: &str) -> AccountPill {
    AccountPill { agent: agent.into(), account: account.into() }
}

fn two() -> Vec<AccountPill> {
    vec![pill("claude", "work"), pill("codex", "System")]
}

fn texts(row: Rect, pills: &[AccountPill]) -> Vec<String> {
    pill_rects(row, pills).into_iter().map(|(_, _, text)| text).collect()
}

#[test]
fn the_pills_sit_in_the_row_one_cell_apart() {
    let rects = pill_rects(ROW, &two());
    assert_eq!(texts(ROW, &two()), [" claude: work ▾ ", " codex: System ▾ "]);
    assert_eq!(rects[0].1, Rect { x: ROW.x + 1, y: ROW.y, width: 16, height: 1 });
    assert_eq!(rects[1].1.x, rects[0].1.right() + 1);
}

#[test]
fn a_pill_is_hit_inside_its_cells_only() {
    let rects = pill_rects(ROW, &two());
    for (i, r, _) in &rects {
        assert_eq!(account_pill_hit(ROW, &two(), Position::new(r.right() - 1, r.y)), Some(*i));
    }
    assert_eq!(account_pill_hit(ROW, &two(), Position::new(rects[0].1.right(), ROW.y)), None);
    assert_eq!(account_pill_hit(ROW, &two(), Position::new(ROW.x + 1, ROW.y + 1)), None);
}

#[test]
fn pills_that_do_not_fit_drop_the_agent_then_the_last_ones() {
    let narrow = |width| Rect { width, ..ROW };
    assert_eq!(texts(narrow(30), &two()), [" work ▾ ", " System ▾ "]);
    let three = [pill("claude", "work"), pill("codex", "Personal"), pill("gemini", "System")];
    assert_eq!(texts(narrow(26), &three), [" work ▾ ", " Personal ▾ "]);
}

#[test]
fn a_long_label_alone_is_cut() {
    let shown = texts(Rect { width: 20, ..ROW }, &[pill("claude", "Everything at work, all day")]);
    assert_eq!(shown.len(), 1);
    assert!(shown[0].ends_with("… ▾ "), "{shown:?}");
}

#[test]
fn nothing_shows_without_room_or_pills() {
    assert_eq!(pill_rects(Rect { width: 6, ..ROW }, &two()), []);
    assert_eq!(pill_rects(Rect { height: 0, ..ROW }, &two()), []);
    assert_eq!(pill_rects(ROW, &[]), []);
}
