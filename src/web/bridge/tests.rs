use super::*;

#[test]
fn the_page_theme_becomes_the_host_theme() {
    let theme = Theme {
        foreground: Some("#e0e0e0".into()),
        background: Some("#101820".into()),
        palette: vec!["#000000".into(), "#cc0000".into(), "bad".into()],
    };

    let host = host_theme(&theme);

    assert_eq!(host.background, Some(RgbColor { r: 0x10, g: 0x18, b: 0x20 }));
    assert_eq!(
        (host.palette[1], host.palette[2], host.palette[16]),
        (Some(RgbColor { r: 0xcc, g: 0, b: 0 }), None, None)
    );
    assert!(host.truecolor);
}

#[test]
fn the_hello_carries_this_build_and_no_images() {
    let hello = hello(48, 0, &Theme::default());

    assert_eq!((hello.build, hello.cols, hello.rows), (protocol::build_id(), 48, 1));
    assert_eq!(size(u16::MAX, u16::MAX), (MAX_COLS, MAX_ROWS));
    assert!(hello.graphics.protocol.is_none());
}

#[test]
fn page_messages_are_read_by_their_type() {
    let hello: FromPage = serde_json::from_str(r#"{"type":"hello","cols":48,"rows":41}"#).expect("hello");
    let resize: FromPage = serde_json::from_str(r#"{"type":"resize","cols":90,"rows":30}"#).expect("resize");

    assert!(matches!(hello, FromPage::Hello { cols: 48, rows: 41, .. }));
    assert!(matches!(resize, FromPage::Resize { cols: 90, rows: 30 }));
}
