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
