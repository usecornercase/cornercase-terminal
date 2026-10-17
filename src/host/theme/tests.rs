use rstest::rstest;

use super::*;

const fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
    RgbColor { r, g, b }
}

fn probe(input: &[u8]) -> ThemeProbe {
    let mut p = ThemeProbe::default();
    p.feed(input);
    p
}

mod parse_color {
    use super::*;

    #[rstest]
    #[case::sixteen_bit("rgb:cccc/dddd/eeee", Some(rgb(0xcc, 0xdd, 0xee)))]
    #[case::eight_bit("rgb:12/34/56", Some(rgb(0x12, 0x34, 0x56)))]
    #[case::four_bit("rgb:f/0/8", Some(rgb(0xff, 0x00, 0x88)))]
    #[case::hash("#123456", Some(rgb(0x12, 0x34, 0x56)))]
    #[case::missing_component("rgb:12/34", None)]
    #[case::not_hex("rgb:zz/00/00", None)]
    #[case::unknown_format("red", None)]
    fn reads_xterm_color_specs(#[case] value: &str, #[case] expected: Option<RgbColor>) {
        assert_eq!(parse_color(value), expected);
    }
}

mod probe {
    use super::*;

    const GHOSTTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;10t\x1b]10;rgb:e0e0/e0e0/e0e0\x1b\\\
            \x1b]11;rgb:2020/2020/2020\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c";
    const FOOT: &[u8] = b"\x1b[6;19;10t\x1b[?1;0;1024S\x1b[?2;0;1000;570S\x1bP>|foot(1.25.0)\x1b\\\
            \x1b[?62;4;22;28;52c";

    fn theme(input: &[u8]) -> HostTheme {
        probe(input).finish().0
    }

    fn replies(input: &[u8]) -> Replies {
        probe(input).finish().1
    }

    #[rstest]
    #[case::st_terminated(b"\x1b]11;rgb:0000/0000/0000\x1b\\")]
    #[case::bel_terminated(b"\x1b]11;rgb:0000/0000/0000\x07")]
    fn reads_the_background(#[case] input: &[u8]) {
        assert_eq!(theme(input).background, Some(rgb(0, 0, 0)));
    }

    #[test]
    fn reads_the_foreground() {
        assert_eq!(theme(b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\").foreground, Some(rgb(255, 255, 255)));
    }

    #[test]
    fn reads_palette_entries() {
        assert_eq!(theme(b"\x1b]4;1;rgb:cc/00/00\x1b\\").palette[1], Some(rgb(0xcc, 0, 0)));
    }

    #[test]
    fn ignores_out_of_range_palette_entries() {
        assert_eq!(theme(b"\x1b]4;300;rgb:cc/00/00\x1b\\"), HostTheme::default());
    }

    #[test]
    fn is_done_after_device_attributes() {
        assert!(probe(b"\x1b]11;rgb:00/00/00\x1b\\\x1b[?62;22c").is_done());
    }

    #[test]
    fn is_not_done_without_device_attributes() {
        assert!(!probe(b"\x1b]11;rgb:00/00/00\x1b\\").is_done());
    }

    #[test]
    fn joins_replies_split_across_reads() {
        let mut p = ThemeProbe::default();
        for chunk in [
            &b"\x1b"[..],
            b"]11;rgb:12",
            b"/34/56\x1b",
            b"\\\x1b[6;1",
            b"9;10t\x1b_Gi=3",
            b"1;OK\x1b",
            b"\\\x1b[?6",
            b"2c",
        ] {
            p.feed(chunk);
        }
        let done = p.is_done();
        let (theme, replies) = p.finish();

        assert_eq!(
            (done, theme.background, replies.cell, replies.kitty.as_deref()),
            (true, Some(rgb(0x12, 0x34, 0x56)), Some(CellSize { width: 10, height: 19 }), Some("OK"))
        );
    }

    #[test]
    fn skips_unrelated_bytes() {
        assert_eq!(theme(b"abc\x1b[A\x1b]11;#000000\x07").background, Some(rgb(0, 0, 0)));
    }

    #[test]
    fn reads_the_terminal_name_and_version() {
        let p = probe(b"\x1b]11;rgb:00/00/00\x1b\\\x1bP>|ghostty 1.2.0\x1b\\\x1b[?62;22c");
        assert_eq!((p.is_done(), p.finish().1.terminal.as_deref()), (true, Some("ghostty 1.2.0")));
    }

    #[rstest]
    #[case::decrqss(b"\x1bP1$r0m\x1b\\\x1b[?62;22c")]
    #[case::xtgettcap(b"\x1bP1+r544E=787465726D2D67686F73747479\x1b\\\x1b[?62;22c")]
    fn has_no_terminal_name_when_the_terminal_does_not_say(#[case] input: &[u8]) {
        assert_eq!(replies(input).terminal, None);
    }

    #[test]
    fn reads_everything_ghostty_answers() {
        let replies = replies(GHOSTTY);

        assert_eq!(
            replies,
            Replies {
                terminal: Some("ghostty 1.3.1".into()),
                attributes: Some(vec![62, 22, 52]),
                kitty: Some("OK".into()),
                cell: Some(CellSize { width: 10, height: 19 }),
                registers: None,
                geometry: None,
            }
        );
    }

    #[test]
    fn reads_the_sixel_limits_foot_answers() {
        let replies = replies(FOOT);

        assert_eq!(
            (replies.registers, replies.geometry, replies.attributes),
            (Some(1024), Some((1000, 570)), Some(vec![62, 4, 22, 28, 52]))
        );
    }

    #[rstest]
    #[case::xterm_without_sixel(b"\x1b[?1;3S\x1b[?2;3S\x1b[?62c")]
    #[case::iterm2_with_an_empty_value(b"\x1b[?1;3;S\x1b[?62c")]
    #[case::tmux_without_a_geometry(b"\x1b[?2;3;0S\x1b[?1;2;4c")]
    fn ignores_sixel_limits_the_terminal_failed_to_give(#[case] input: &[u8]) {
        let replies = replies(input);

        assert_eq!((replies.registers, replies.geometry), (None, None));
    }

    #[test]
    fn keeps_empty_device_attributes_out() {
        assert_eq!(replies(b"\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c").attributes, Some(vec![62, 52]));
    }

    #[rstest]
    #[case::kitty_error(
        b"\x1b_Gi=31;ENOTSUPPORTED:unicode placeholders\x1b\\",
        Some("ENOTSUPPORTED:unicode placeholders")
    )]
    #[case::another_id(b"\x1b_Gi=7;OK\x1b\\", None)]
    #[case::not_kitty(b"\x1b_hello\x1b\\", None)]
    fn reads_the_answer_to_its_own_kitty_query(#[case] input: &[u8], #[case] expected: Option<&str>) {
        assert_eq!(replies(input).kitty.as_deref(), expected);
    }

    #[rstest]
    #[case::window_in_pixels(b"\x1b[4;570;1000t")]
    #[case::window_in_cells(b"\x1b[8;30;100t")]
    fn takes_only_the_cell_size_report_as_a_cell(#[case] input: &[u8]) {
        assert_eq!(replies(input).cell, None);
    }

    #[rstest]
    #[case::secondary_attributes(b"\x1b[>1;10;0c")]
    #[case::mode_report(b"\x1b[?2026;2$y")]
    #[case::konsole_cell_report(b"\x1b]1337;ReportCellSize=19.0;10.0;1.0\x07")]
    fn other_reports_do_not_end_the_probe(#[case] input: &[u8]) {
        let mut p = probe(input);
        p.feed(b"\x1b[?62;22c");

        assert_eq!(p.finish().1.attributes, Some(vec![62, 22]));
    }

    #[rstest]
    #[case::ghostty(GHOSTTY)]
    #[case::foot(FOOT)]
    fn reads_the_same_however_the_replies_are_split(#[case] stream: &[u8]) {
        let whole = replies(stream);
        for at in 1..stream.len() {
            let mut p = ThemeProbe::default();
            p.feed(&stream[..at]);
            p.feed(&stream[at..]);

            assert_eq!(p.finish().1, whole, "split at {at}");
        }
    }

    #[rstest]
    #[case::alt_underscore(b"\x1b_")]
    #[case::alt_p(b"\x1bP")]
    #[case::alt_bracket(b"\x1b]")]
    #[case::alt_underscore_and_a_letter(b"\x1b_x")]
    fn a_key_typed_during_the_probe_does_not_swallow_the_replies(#[case] key: &[u8]) {
        assert_eq!(replies(&[key, GHOSTTY].concat()), replies(GHOSTTY));
    }

    #[test]
    fn reads_what_the_terminal_outside_tmux_answers_through_it() {
        let replies = replies(b"\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c");

        assert_eq!(
            (replies.terminal.as_deref(), replies.kitty.as_deref(), replies.attributes),
            (Some("kitty(0.45.0)"), Some("OK"), Some(vec![62, 52]))
        );
    }
}

mod query {
    use super::*;

    #[test]
    fn asks_for_colors_and_ends_with_device_attributes() {
        let q = HostTheme::query(false);
        assert!(q.contains("\x1b]10;?\x1b\\\x1b]11;?\x1b\\") && q.ends_with("\x1b]4;15;?\x1b\\\x1b[>q\x1b[c"));
    }

    #[test]
    fn asks_only_for_the_sixteen_themed_colors() {
        let query = HostTheme::query(true);
        let indices: Vec<usize> = query
            .split("\x1b]4;")
            .skip(1)
            .map(|part| part.split(';').next().expect("palette index").parse().expect("numeric index"))
            .collect();
        assert_eq!(indices, (0..16).collect::<Vec<_>>());
    }

    #[rstest]
    #[case::ghostty("rgb:ffff/ffff/ffff", 862)]
    #[case::kitty("rgb:ffff/ffff/ffff", 862)]
    #[case::iterm2_short_rgb("rgb:ff/ff/ff", 754)]
    #[case::hash_rgb("#ffffff", 664)]
    fn the_reply_batch_fits_the_smallest_tty_queue(#[case] color: &str, #[case] size: usize) {
        let query = HostTheme::query(true);
        let mut reply = String::from(
            "\x1b_Gi=31;ENOTSUPPORTED:unicode placeholders\x1b\\\x1b[6;512;256t\x1b[?1;0;65536S\x1b[?2;0;65536;65536S",
        );
        for part in query.split("\x1b]").skip(1) {
            let osc = part.split("\x1b\\").next().expect("OSC body").trim_end_matches('?');
            write!(reply, "\x1b]{osc}{color}\x1b\\").expect("color reply");
        }
        write!(reply, "\x1bP>|{}\x1b\\\x1b[?62;1;2;4;6;9;15;18;21;22;28;29;42;52c", "v".repeat(256))
            .expect("long terminal version and DA1");
        assert_eq!(reply.len(), size);
        assert!(reply.len() <= 1022, "{} bytes would fill the tty queue", reply.len());
        let mut probe = ThemeProbe::default();
        probe.feed(reply.as_bytes());
        assert!(probe.is_done());
        let (theme, _) = probe.finish();
        assert!(theme.palette[..16].iter().all(Option::is_some));
        assert!(theme.palette[16..].iter().all(Option::is_none));
    }

    #[test]
    fn asks_for_the_cell_size_and_the_sixel_limits_first() {
        assert!(HostTheme::query(false).starts_with("\x1b[16t\x1b[?1;1;0S\x1b[?2;1;0S\x1b]10;?"));
    }

    #[rstest]
    #[case::asked(true, true)]
    #[case::not_asked(false, false)]
    fn sends_the_kitty_query_only_when_asked(#[case] kitty: bool, #[case] expected: bool) {
        assert_eq!(HostTheme::query(kitty).starts_with(detect::KITTY_QUERY), expected);
        assert_eq!(HostTheme::query(kitty).contains("\x1b_G"), expected);
    }
}

mod is_light {
    use super::*;

    #[rstest]
    #[case::unknown(None, None)]
    #[case::black(Some(rgb(0, 0, 0)), Some(false))]
    #[case::white(Some(rgb(255, 255, 255)), Some(true))]
    #[case::solarized_light(Some(rgb(0xfd, 0xf6, 0xe3)), Some(true))]
    #[case::solarized_dark(Some(rgb(0x00, 0x2b, 0x36)), Some(false))]
    fn follows_background_luminance(#[case] background: Option<RgbColor>, #[case] expected: Option<bool>) {
        assert_eq!(HostTheme { background, ..HostTheme::default() }.is_light(), expected);
    }
}

mod muted_is_readable {
    use super::*;

    #[rstest]
    #[case::nothing_known(None, None, true)]
    #[case::palette_without_background(None, Some(rgb(0x2b, 0x2b, 0x2a)), true)]
    #[case::warp_adeberry_without_palette(Some(rgb(0x1d, 0x20, 0x22)), None, false)]
    #[case::warp_adeberry_as_drawn(Some(rgb(0x1d, 0x20, 0x22)), Some(rgb(0x2b, 0x2b, 0x2a)), false)]
    #[case::solarized_dark(Some(rgb(0x00, 0x2b, 0x36)), Some(rgb(0x00, 0x2b, 0x36)), false)]
    #[case::nord(Some(rgb(0x2e, 0x34, 0x40)), Some(rgb(0x4c, 0x56, 0x6a)), false)]
    #[case::one_dark(Some(rgb(0x28, 0x2c, 0x34)), Some(rgb(0x66, 0x66, 0x66)), true)]
    #[case::dracula(Some(rgb(0x28, 0x2a, 0x36)), Some(rgb(0x62, 0x72, 0xa4)), true)]
    #[case::solarized_light(Some(rgb(0xfd, 0xf6, 0xe3)), Some(rgb(0x00, 0x2b, 0x36)), true)]
    fn needs_the_bright_black_to_stand_out_from_a_known_background(
        #[case] background: Option<RgbColor>,
        #[case] bright_black: Option<RgbColor>,
        #[case] expected: bool,
    ) {
        let mut theme = HostTheme { background, ..HostTheme::default() };
        theme.palette[8] = bright_black;

        assert_eq!(theme.muted_is_readable(), expected);
    }
}

mod soft_is_readable {
    use super::*;

    #[rstest]
    #[case::nothing_known(None, None, true)]
    #[case::tokyo_night(Some(rgb(0x1a, 0x1b, 0x26)), Some(rgb(0xa9, 0xb1, 0xd6)), true)]
    #[case::github_light(Some(rgb(0xff, 0xff, 0xff)), Some(rgb(0x6e, 0x77, 0x81)), true)]
    #[case::solarized_light(Some(rgb(0xfd, 0xf6, 0xe3)), Some(rgb(0xee, 0xe8, 0xd5)), false)]
    #[case::two_to_one_is_not_enough(Some(rgb(0x00, 0x00, 0x00)), Some(rgb(0x59, 0x59, 0x59)), false)]
    #[case::white_unknown(Some(rgb(0x1a, 0x1b, 0x26)), None, false)]
    #[case::readable_on_the_background_only(Some(rgb(0xff, 0xff, 0xff)), Some(rgb(0x94, 0x94, 0x94)), false)]
    fn needs_the_white_to_stand_out_three_to_one_from_the_background_and_the_surface(
        #[case] background: Option<RgbColor>,
        #[case] white: Option<RgbColor>,
        #[case] expected: bool,
    ) {
        let mut theme = HostTheme { background, ..HostTheme::default() };
        theme.palette[7] = white;
        let surface = if theme.is_light() == Some(true) { rgb(0xe4, 0xe4, 0xe4) } else { rgb(0x30, 0x30, 0x30) };

        assert_eq!(theme.soft_is_readable(surface), expected);
    }
}

mod wire {
    use super::*;

    #[test]
    fn round_trips_through_serde() {
        let mut theme = HostTheme { background: Some(rgb(1, 2, 3)), ..HostTheme::default() };
        theme.palette[200] = Some(rgb(4, 5, 6));

        let bytes = postcard::to_stdvec(&theme).expect("encode theme");

        assert_eq!(postcard::from_bytes::<HostTheme>(&bytes).expect("decode theme"), theme);
    }
}
