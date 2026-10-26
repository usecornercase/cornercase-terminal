use super::*;

const URL: &str = "https://mac.tail1234.ts.net:8443/pair?t=0123456789abcdef0123456789abcdef";

#[test]
fn the_code_is_square_with_a_quiet_zone() {
    let (width, _) = modules(URL).expect("a code");
    let code = code(URL);

    assert_eq!(code.len(), (width + 2 * QUIET).div_ceil(2));
    assert!(code.iter().all(|line| line.spans.len() == width + 2 * QUIET));
    assert_eq!(code[0].spans[0].style.fg, Some(LIGHT));
}

#[test]
fn a_window_too_short_shows_the_address_instead_of_the_code() {
    let remote = RemoteAccess { title: TITLE, url: Some(URL.into()), code: code(URL), lines: vec!["hint".into()] };

    let short = body(&remote, 60, 10, Color::Reset);
    let tall = body(&remote, 100, 40, Color::Reset);

    assert_eq!(short[0].to_string(), TOO_SMALL);
    assert!(tall.len() > 20 && tall.iter().any(|line| line.to_string() == URL));
}
