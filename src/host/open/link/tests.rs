use rstest::rstest;

use super::*;

const NOT_WEB: &str = "it is not an http or https address";
const CHARACTER: &str = "it holds a character a web address cannot have";
const NO_HOST: &str = "it has no host";

#[rstest]
#[case::plain_https("https://example.com", "example.com")]
#[case::http_with_a_port("http://localhost:3000/", "localhost")]
#[case::an_at_in_the_path("https://example.com/@someone/post", "example.com")]
#[case::a_query_and_a_fragment("https://docs.rs/x?q=a&b=c#part", "docs.rs")]
#[case::an_ipv6_address_with_a_port("http://[::1]:8080/", "[::1]")]
#[case::a_query_right_after_the_host("https://example.com?q=1", "example.com")]
fn an_address_a_window_may_open_gives_its_host(#[case] url: &str, #[case] expected: &str) {
    assert_eq!(host(url), Ok(expected));
}

#[rstest]
#[case::a_file("file:///etc/passwd", NOT_WEB)]
#[case::a_script("javascript:alert(1)", NOT_WEB)]
#[case::another_app("vscode://x", NOT_WEB)]
#[case::an_upper_case_scheme("HTTPS://x.dev", NOT_WEB)]
#[case::text_before_the_scheme("-https://x.dev", NOT_WEB)]
#[case::a_space("https://x.dev/a b", CHARACTER)]
#[case::a_control_character("https://x.dev/\u{7}", CHARACTER)]
#[case::a_quote("https://x.dev/\"", CHARACTER)]
#[case::a_backslash("https://x.dev\\@evil.example/", CHARACTER)]
#[case::a_host_that_is_not_ascii("https://exämple.com/", CHARACTER)]
#[case::a_user_name("https://good.com@evil.example/", "it carries a user name")]
#[case::no_host("https:///path", NO_HOST)]
#[case::only_a_port("https://:80/", NO_HOST)]
#[case::a_port_that_is_not_a_number("https://x.dev:http/", NO_HOST)]
#[case::a_host_that_starts_with_a_dot("https://.x.dev/", NO_HOST)]
#[case::empty_brackets("https://[]/", NO_HOST)]
fn an_address_a_window_must_not_open_says_why(#[case] url: &str, #[case] reason: &str) {
    assert_eq!(host(url), Err(reason));
}

#[test]
fn the_longest_address_allowed_opens() {
    let url = format!("https://example.com/{}", "a".repeat(MAX_LINK - 20));
    assert_eq!(url.len(), MAX_LINK);
    assert_eq!(host(&url), Ok("example.com"));
}

#[test]
fn an_address_over_the_limit_is_too_long() {
    let url = format!("https://example.com/{}", "a".repeat(MAX_LINK - 19));
    assert_eq!(url.len(), MAX_LINK + 1);
    assert_eq!(host(&url), Err("it is too long"));
}
