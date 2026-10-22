use super::*;

#[test]
fn the_page_loads_only_assets_it_serves() {
    for name in ["/xterm.css", "/app.css", "/xterm.js", "/addon-fit.js", "/app.js"] {
        assert!(INDEX.contains(&format!("\"{name}\"")), "{name} is not loaded by the page");
        assert!(asset(name).is_some(), "{name} is not served");
    }
}

#[test]
fn the_page_fits_a_phone_screen() {
    assert!(INDEX.contains(r#"<meta name="viewport" content="width=device-width"#));
}

#[test]
fn unknown_paths_are_no_asset() {
    assert!(asset("/secrets.json").is_none());
}

fn sent(name: &str) -> (String, Vec<u8>) {
    let mut wire = Vec::new();
    asset(name).unwrap_or_else(|| panic!("{name} is not served")).send(&mut wire).expect("send");
    let split = wire.windows(4).position(|w| w == b"\r\n\r\n").expect("a head") + 4;
    (String::from_utf8_lossy(&wire[..split]).into_owned(), wire[split..].to_vec())
}

#[test]
fn the_symbols_font_is_served_as_woff2() {
    let (head, body) = sent("/symbols.woff2");
    assert!(head.starts_with("HTTP/1.1 200 OK") && head.contains("Content-Type: font/woff2"));
    assert_eq!(body.get(..4), Some(b"wOF2".as_slice()));
}

#[test]
fn the_symbols_licence_is_served() {
    let text = String::from_utf8(sent("/symbols-LICENSE.txt").1).expect("text");
    assert!(text.contains("CC BY 4.0") && text.contains("Apache License") && text.contains("MIT"));
}

#[test]
fn every_url_in_app_css_is_served() {
    let css = include_str!("../assets/app.css");
    let urls: Vec<&str> = css.split("url('").skip(1).filter_map(|rest| rest.split('\'').next()).collect();
    assert_eq!(urls, ["/symbols.woff2"]);
    for url in urls {
        assert!(asset(url).is_some(), "{url} is not served");
    }
}
