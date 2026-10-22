use rstest::rstest;

use super::*;

fn glyph_of(icon: Icon) -> char {
    icon.glyph.chars().next().expect("a glyph")
}

fn colour_name(colour: Option<Color>) -> String {
    colour.map_or_else(|| "soft".into(), |c| format!("{c:?}").to_lowercase())
}

fn lines(kind: &str, table: &[(&[&str], Icon)]) -> Vec<String> {
    table
        .iter()
        .flat_map(|(keys, icon)| {
            keys.iter().map(move |key| {
                format!("{kind} {} {:x} {}", key.to_lowercase(), u32::from(glyph_of(*icon)), colour_name(icon.colour))
            })
        })
        .collect()
}

#[rstest]
#[case::lock_beats_json("package-lock.json", "package-lock.json", '\u{e672}')]
#[case::cargo_beats_toml("Cargo.toml", "Cargo.toml", '\u{e68b}')]
#[case::upper_case_rust("MAIN.RS", "main.rs", '\u{e68b}')]
#[case::upper_case_jpg("Photo.JPG", "photo.jpg", '\u{e60d}')]
#[case::name_ignores_case("makefile", "Makefile", '\u{e673}')]
#[case::upper_case_name("MAKEFILE", "Makefile", '\u{e673}')]
#[case::readme_lower_case("readme.md", "README.md", '\u{e66a}')]
#[case::gitignore(".gitignore", ".gitignore", '\u{e65d}')]
#[case::env_file(".env", ".env", '\u{e615}')]
#[case::shell_rc(".bashrc", ".bashrc", '\u{e691}')]
#[case::last_extension_counts("archive.tar.gz", "x.zip", '\u{e6aa}')]
#[case::double_extension("app.test.ts", "x.ts", '\u{e628}')]
fn a_known_file_gets_its_icon(#[case] name: &str, #[case] same_as: &str, #[case] glyph: char) {
    assert_eq!((glyph_of(file(name)), file(name)), (glyph, file(same_as)));
}

#[rstest]
#[case::unknown_extension("notes.xyz")]
#[case::no_extension("Procfile")]
#[case::unknown_dotfile(".unknownrc")]
fn an_unknown_type_gets_the_generic_icon(#[case] name: &str) {
    assert_eq!(file(name), GENERIC);
}

#[test]
fn folders_open_and_close() {
    assert_eq!((folder(false).glyph, folder(true).glyph), ("\u{e5ff}", "\u{e5fe}"));
}

#[test]
fn paths_use_their_file_name() {
    assert_eq!((file_at("src/ui/files.rs"), file_at("Cargo.toml")), (file("files.rs"), file("Cargo.toml")));
}

#[test]
fn the_table_matches_the_shared_fixture() {
    let mut text = vec![
        format!("folder closed {:x} blue", u32::from(glyph_of(FOLDER))),
        format!("folder open {:x} blue", u32::from(glyph_of(FOLDER_OPEN))),
        format!("file - {:x} soft", u32::from(glyph_of(GENERIC))),
    ];
    text.extend(lines("name", NAMES));
    text.extend(lines("ext", EXTENSIONS));
    assert_eq!(text.join("\n") + "\n", crate::test_util::fixture!("icons/table.txt"));
}

#[test]
fn every_colour_is_an_ansi_one_or_the_soft_grey() {
    let allowed = [Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Magenta, Color::Cyan];
    let all = NAMES.iter().chain(EXTENSIONS).map(|(_, icon)| icon).chain([&FOLDER, &FOLDER_OPEN, &GENERIC]);
    let bad: Vec<_> = all.filter(|icon| icon.colour.is_some_and(|c| !allowed.contains(&c))).collect();
    assert_eq!(bad, Vec::<&Icon>::new());
}
