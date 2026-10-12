use libghostty_vt::style::RgbColor;
use rstest::rstest;

use super::*;
use crate::host_theme::HostTheme;

#[rstest]
#[case::unknown_background(None, Color::DarkGray)]
#[case::dark_without_palette(Some(RgbColor { r: 0x1d, g: 0x20, b: 0x22 }), Color::Indexed(243))]
#[case::light_without_palette(Some(RgbColor { r: 0xff, g: 0xff, b: 0xff }), Color::Indexed(245))]
fn swaps_a_bright_black_that_may_not_show_for_a_fixed_grey(
    #[case] background: Option<RgbColor>,
    #[case] expected: Color,
) {
    assert_eq!(muted(&HostTheme { background, ..HostTheme::default() }), expected);
}

#[test]
fn keeps_a_bright_black_that_shows() {
    let mut theme = HostTheme { background: Some(RgbColor { r: 0x28, g: 0x2a, b: 0x36 }), ..HostTheme::default() };
    theme.palette[8] = Some(RgbColor { r: 0x62, g: 0x72, b: 0xa4 });

    assert_eq!(muted(&theme), Color::DarkGray);
}
