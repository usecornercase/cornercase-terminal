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
        .chain([ISSUES, FLOW, WORKTREES, RECAP, HELP, MORE, DOT, MANY])
        .chain(COUNTS);
    let kinds = NAMES.iter().chain(EXTENSIONS).map(|(_, icon)| icon.glyph);
    let kinds = kinds.chain(lucide::LUCIDE.iter().map(|(_, glyph)| *glyph));
    let types = [folder(true).glyph, folder(false).glyph, file("").glyph];
    let drawn: BTreeSet<u32> = panel.chain(kinds).chain(types).flat_map(codes).collect();
    let listed: BTreeSet<u32> = crate::test_util::fixture!("icons/symbols.txt")
        .lines()
        .map(|line| u32::from_str_radix(line.trim_start_matches("U+"), 16).expect("a code point"))
        .collect();
    assert_eq!(drawn, listed);
}

mod a_division_icon {
    use super::*;

    #[test]
    fn is_drawn_for_each_lucide_name_agency_agents_uses() {
        let names = [
            "GraduationCap",
            "PenTool",
            "Code",
            "DollarSign",
            "Gamepad2",
            "Map",
            "Stethoscope",
            "Megaphone",
            "Target",
            "Box",
            "ClipboardList",
            "Search",
            "TrendingUp",
            "ShieldCheck",
            "Boxes",
            "Sparkles",
            "LifeBuoy",
            "FlaskConical",
        ];

        let missing: Vec<&str> = names.into_iter().filter(|name| lucide(name).is_none()).collect();

        assert_eq!(missing, Vec::<&str>::new());
    }

    #[test]
    fn is_nothing_for_a_name_it_does_not_know() {
        assert_eq!(lucide("Rocket"), None);
    }

    #[test]
    fn tint_reads_only_a_hex_colour() {
        assert_eq!(tint("#3B82F6"), Some(Color::Rgb(0x3b, 0x82, 0xf6)));
        assert_eq!((tint("blue"), tint("#3B82")), (None, None));
    }
}
