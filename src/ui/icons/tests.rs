use std::collections::BTreeSet;

use super::kinds::{EXTENSIONS, NAMES};
use super::*;

fn codes(glyph: &str) -> impl Iterator<Item = u32> + '_ {
    glyph.chars().map(u32::from).filter(|c| (0xe000..0xf8ff).contains(c) || *c >= 0xf0000)
}

#[test]
fn the_symbols_subset_holds_every_glyph() {
    let panel = [&NERD.menu, &NERD.changes, &NERD.files, &NERD.todo, &NERD.search, &NERD.filter, &NERD.names]
        .into_iter()
        .chain([&NERD.said, &NERD.full_screen, &NERD.restore])
        .copied()
        .chain([ISSUES, FLOW, UPDATE, REMOTE, USAGE, HELP, SETTINGS, QUIT, DOT, MANY])
        .chain(COUNTS);
    let kinds = NAMES.iter().chain(EXTENSIONS).map(|(_, icon)| icon.glyph);
    let types = [folder(true).glyph, folder(false).glyph, file("").glyph];
    let drawn: BTreeSet<u32> = panel.chain(kinds).chain(types).flat_map(codes).collect();
    let listed: BTreeSet<u32> = crate::test_util::fixture!("icons/symbols.txt")
        .lines()
        .map(|line| u32::from_str_radix(line.trim_start_matches("U+"), 16).expect("a code point"))
        .collect();
    assert_eq!(drawn, listed);
}
