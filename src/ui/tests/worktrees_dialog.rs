use ratatui::style::Color;

use crate::worktrees::Order;

use super::*;

const MERGED: Color = Color::Indexed(99);

const BIG: (u16, u16) = (120, 34);

const SMALL: (u16, u16) = (60, 28);

fn row(project: &str, branch: &str, folder: &str, age: &str) -> WorktreeRow {
    WorktreeRow {
        ticked: false,
        tickable: true,
        project: project.into(),
        branch: branch.into(),
        folder: folder.into(),
        age: age.into(),
        marks: Vec::new(),
        changes: true,
        free: true,
        size: "…".into(),
        builds: None,
        status: None,
    }
}

fn rows() -> Vec<WorktreeRow> {
    vec![
        WorktreeRow {
            marks: vec![("prunable".into(), Color::DarkGray)],
            changes: false,
            free: false,
            size: String::new(),
            ..row("web-shop", "feat/old-spike", "~/.cornercase/worktrees/web-shop/old-spike", "41 days ago")
        },
        WorktreeRow {
            ticked: true,
            size: "2.4 GB".into(),
            builds: Some("2.1 GB builds".into()),
            marks: vec![("merged".into(), MERGED), ("↑2".into(), Color::Cyan)],
            ..row("web-shop", "feat/gift-cards", "~/.cornercase/worktrees/web-shop/gift-cards", "3 days ago")
        },
        WorktreeRow {
            size: "310 MB".into(),
            builds: Some("280 MB builds".into()),
            marks: vec![
                ("●4".into(), Color::Yellow),
                ("locked".into(), Color::Yellow),
                ("claude".into(), Color::Yellow),
                ("1 outside".into(), Color::Yellow),
            ],
            ..row("orders-api", "fix/pagination", "~/.cornercase/worktrees/orders-api/pagination", "2h ago")
        },
        WorktreeRow {
            size: "12 MB".into(),
            free: false,
            marks: vec![("no commits".into(), Color::DarkGray), ("open".into(), Color::Gray)],
            ..row("web-shop", "feat/empty-cart", "~/.cornercase/worktrees/web-shop/empty-cart", "5 min ago")
        },
        WorktreeRow {
            tickable: false,
            size: "88 MB".into(),
            free: false,
            status: Some("owned by a flow run".into()),
            ..row("payments", "full-issue--coder", "~/.cornercase/worktrees/payments/coder", "just now")
        },
    ]
}

fn worktrees() -> Worktrees {
    Worktrees {
        title: "Worktrees".into(),
        filter: String::new(),
        rows: rows(),
        selected: Some(2),
        scroll: 0,
        ticked: 1,
        free: 1,
        order: Order::Safe,
        total: None,
        note: None,
        confirm: None,
        compact: false,
    }
}

fn confirm(branches: bool, checking: bool) -> ConfirmView {
    let red = Style::default().fg(Color::Red);
    ConfirmView {
        lines: vec![
            Line::from("web-shop › feat/gift-cards"),
            Line::from("  2 commits not pushed, a teardown runs first"),
            Line::from("orders-api › fix/pagination"),
            Line::from("  4 files changed, locked by a pid that is gone"),
            Line::from(Span::styled("  claude working in 2 tabs, they stop", red)),
            Line::from(Span::styled("  feat/gift-cards: its commits will be lost", red)),
        ],
        scroll: 0,
        branches,
        checking,
        kind: Purpose::Remove,
    }
}

fn shown(w: Worktrees) -> View<'static> {
    View { overlay: Some(Overlay::Worktrees(Box::new(w))), ..view(&["web-shop"]) }
}

fn snapshot(w: Worktrees, (width, height): (u16, u16)) -> String {
    format!("{}", render_sized(&shown(w), width, height).backend())
}

fn at(w: &Worktrees, size: (u16, u16), i: usize) -> Rect {
    let list = list_rows(worktrees_area(screen(size), w.compact))[1];
    list_item_by(list, w.rows.len(), w.scroll, i, if w.compact { 3 } else { 1 })
}

fn screen((width, height): (u16, u16)) -> Rect {
    Rect::new(0, 0, width, height)
}

#[test]
fn worktrees_list_checking() {
    let w = Worktrees { note: Some(Note::Busy("checking 3 of 7…")), ..worktrees() };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_list_sorted() {
    let w = Worktrees { selected: None, ticked: 0, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_list_narrow() {
    insta::assert_snapshot!(snapshot(worktrees(), (90, 14)));
}

#[test]
fn worktrees_confirm_with_risks() {
    let w = Worktrees { confirm: Some(confirm(false, false)), ..worktrees() };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_confirm_deleting_branches() {
    let w =
        Worktrees { note: Some(Note::Busy("checking 1 of 2…")), confirm: Some(confirm(true, true)), ..worktrees() };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_compact() {
    let w = Worktrees { compact: true, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, SMALL));
}

#[test]
fn the_dialog_border_is_at_its_area() {
    let v = shown(worktrees());
    let r = v.overlay.as_ref().and_then(|o| o.area(AREA)).expect("an area");
    let t = render(&v);
    let cell = |x, y| t.backend().buffer()[(x, y)].symbol().to_string();
    assert_eq!((cell(r.x, r.y), cell(r.right() - 1, r.bottom() - 1)), ("╭".to_string(), "╯".to_string()));
}

#[test]
fn the_compact_dialog_takes_the_whole_screen() {
    assert_eq!(worktrees_area(screen(SMALL), true), screen(SMALL));
    assert_eq!(worktrees_area(screen(SMALL), false), settings_area(screen(SMALL)));
}

#[test]
fn ticks_hit_their_row_checkbox() {
    let w = worktrees();
    let r = at(&w, BIG, 1);
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(r.x + 1, r.y)), Some(WorktreesHit::Tick(1)));
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(r.x + 10, r.y)), Some(WorktreesHit::Row(1)));
    let locked = at(&w, BIG, 4);
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(locked.x + 1, locked.y)), Some(WorktreesHit::Row(4)));
}

#[test]
fn changes_hits_only_on_rows_that_have_them() {
    let w = worktrees();
    for (i, expected) in [(0, WorktreesHit::Row(0)), (1, WorktreesHit::Changes(1))] {
        let r = at(&w, BIG, i);
        assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(r.right() - 6, r.y)), Some(expected));
    }
}

#[test]
fn changes_is_drawn_only_on_the_hovered_row_in_wide_mode() {
    let r = at(&worktrees(), BIG, 1);
    let idle = render_sized(&shown(worktrees()), BIG.0, BIG.1);
    assert!(!row_text(&idle, r).contains("changes"));
    let lit = View { hover: Some(Position::new(r.x + 10, r.y)), ..shown(worktrees()) };
    assert!(row_text(&render_sized(&lit, BIG.0, BIG.1), r).contains("changes"));
}

#[test]
fn changes_is_always_drawn_in_compact_mode() {
    let w = Worktrees { compact: true, ..worktrees() };
    let r = at(&w, SMALL, 1);
    let t = render_sized(&shown(w), SMALL.0, SMALL.1);
    assert!(row_text(&t, Rect { y: r.y + 2, height: 1, ..r }).contains("changes"));
}

#[test]
fn remove_is_disabled_without_ticks_and_while_checking() {
    let area = worktrees_area(screen(BIG), false);
    let pos = Position::new(area.right() - 14, area.bottom() - 2);
    assert_eq!(worktrees_hit(screen(BIG), &worktrees(), pos), Some(WorktreesHit::Remove));
    let none = Worktrees { ticked: 0, ..worktrees() };
    assert_ne!(worktrees_hit(screen(BIG), &none, pos), Some(WorktreesHit::Remove));
    let checking = Worktrees { confirm: Some(confirm(false, true)), ..worktrees() };
    assert_ne!(worktrees_hit(screen(BIG), &checking, pos), Some(WorktreesHit::Remove));
}

#[test]
fn the_confirmation_has_back_remove_and_the_branch_checkbox() {
    let area = worktrees_area(screen(BIG), false);
    let w = Worktrees { confirm: Some(confirm(false, false)), ..worktrees() };
    let y = area.bottom() - 2;
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(area.right() - 14, y)), Some(WorktreesHit::Back));
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(area.right() - 5, y)), Some(WorktreesHit::Remove));
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(area.x + 6, y - 2)), Some(WorktreesHit::Branches));
}

#[test]
fn the_filter_field_is_hit() {
    let area = worktrees_area(screen(BIG), false);
    let pos = Position::new(area.x + 4, area.y + 1);
    assert_eq!(worktrees_hit(screen(BIG), &worktrees(), pos), Some(WorktreesHit::Filter));
}

#[test]
fn worktrees_list_scrolled() {
    let many = (0..9).map(|i| row("web-shop", &format!("feat/branch-{i}"), "~/wt/web-shop/x", "3 days ago")).collect();
    let w = Worktrees { rows: many, scroll: 2, selected: None, ticked: 0, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, (70, 12)));
}

#[test]
fn worktrees_list_empty() {
    let w = Worktrees { rows: Vec::new(), selected: None, ticked: 0, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, (70, 12)));
}

#[test]
fn worktrees_list_no_matches() {
    let w = Worktrees { rows: Vec::new(), filter: "zzz".into(), selected: None, ticked: 0, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, (70, 12)));
}

#[test]
fn worktrees_compact_phone() {
    let w = Worktrees { compact: true, ..worktrees() };
    insta::assert_snapshot!(snapshot(w, (46, 30)));
}

fn buttons_y() -> u16 {
    worktrees_area(screen(BIG), false).bottom() - 2
}

#[test]
fn worktrees_list_measuring() {
    let measuring = rows().into_iter().map(|r| WorktreeRow { size: "…".into(), builds: None, ..r }).collect();
    let w = Worktrees { rows: measuring, note: Some(Note::Info("measuring 2 of 5…".into())), ..worktrees() };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_list_by_size() {
    let w = Worktrees {
        order: Order::Size,
        total: Some(("3.2 GB in 5 worktrees · 2.4 GB of builds idle 7+ days".into(), "3.2 GB · 2.4 GB idle".into())),
        ..worktrees()
    };
    insta::assert_snapshot!(snapshot(w, BIG));
}

#[test]
fn worktrees_free_confirm() {
    let yellow = Style::default().fg(Color::Yellow);
    let bold = Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow);
    let free = ConfirmView {
        lines: vec![
            Line::from(Span::styled("web-shop › feat/gift-cards", bold)),
            Line::from("  frees 2.1 GB: node_modules 1.9 GB, packages/web/.next 180 MB"),
            Line::from(Span::styled("  claude running in it; it may still use these folders", yellow)),
            Line::from(Span::styled("  .venv/src/foo is a git checkout; its changes go too", yellow)),
        ],
        scroll: 0,
        branches: false,
        checking: false,
        kind: Purpose::Free { label: "free 2.1 GB".into() },
    };
    insta::assert_snapshot!(snapshot(Worktrees { confirm: Some(free), ..worktrees() }, BIG));
}

#[test]
fn the_free_step_has_no_branch_checkbox_and_its_button_waits_while_checking() {
    let area = worktrees_area(screen(BIG), false);
    let free =
        |checking| ConfirmView { kind: Purpose::Free { label: "free 2.1 GB".into() }, ..confirm(false, checking) };
    let w = Worktrees { confirm: Some(free(false)), ..worktrees() };
    let y = buttons_y();
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(area.right() - 5, y)), Some(WorktreesHit::Remove));
    assert_ne!(worktrees_hit(screen(BIG), &w, Position::new(area.x + 6, y - 2)), Some(WorktreesHit::Branches));
    let waiting = Worktrees { confirm: Some(free(true)), ..worktrees() };
    assert_ne!(worktrees_hit(screen(BIG), &waiting, Position::new(area.right() - 5, y)), Some(WorktreesHit::Remove));
}

#[test]
fn the_row_button_hits_only_rows_that_can_free() {
    let w = worktrees();
    let (free, none) = (at(&w, BIG, 1), at(&w, BIG, 3));
    let x = free.right() - 20;
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(x, free.y)), Some(WorktreesHit::Free(1)));
    assert_eq!(worktrees_hit(screen(BIG), &w, Position::new(x, none.y)), Some(WorktreesHit::Row(3)));
}

#[test]
fn the_row_button_is_drawn_only_on_the_hovered_row_in_wide_mode_and_always_in_compact() {
    let r = at(&worktrees(), BIG, 1);
    let idle = render_sized(&shown(worktrees()), BIG.0, BIG.1);
    assert!(!row_text(&idle, r).contains("free builds"));
    let lit = View { hover: Some(Position::new(r.x + 10, r.y)), ..shown(worktrees()) };
    assert!(row_text(&render_sized(&lit, BIG.0, BIG.1), r).contains("free builds"));
    let w = Worktrees { compact: true, ..worktrees() };
    let c = at(&w, SMALL, 1);
    let t = render_sized(&shown(w), SMALL.0, SMALL.1);
    assert!(row_text(&t, Rect { y: c.y + 2, height: 1, ..c }).contains("free builds"));
}

#[test]
fn the_bottom_free_button_is_disabled_at_zero_and_the_tabs_set_the_order() {
    let area = worktrees_area(screen(BIG), false);
    let y = buttons_y();
    let free = Worktrees { free: 2, ..worktrees() };
    let none = Worktrees { free: 0, ..worktrees() };
    let hit = |w: &Worktrees, x: u16, y: u16| worktrees_hit(screen(BIG), w, Position::new(x, y));
    let x = area.right() - 27;
    assert_eq!(hit(&free, x, y), Some(WorktreesHit::FreeTicked));
    assert_ne!(hit(&none, x, y), Some(WorktreesHit::FreeTicked));
    let tabs_y = area.y + 1;
    assert_eq!(hit(&none, area.right() - 4, tabs_y), Some(WorktreesHit::Order(Order::Size)));
    assert_eq!(hit(&none, area.right() - 20, tabs_y), Some(WorktreesHit::Order(Order::Safe)));
}
