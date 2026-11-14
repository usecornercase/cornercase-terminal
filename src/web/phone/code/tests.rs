use super::*;
use crate::ui::remote_access::code;

fn routes(list: &[&str]) -> Vec<SocketAddr> {
    list.iter().map(|text| text.parse().expect("an address")).collect()
}

#[test]
fn two_lan_routes_and_a_tailscale_one_with_a_name() {
    let found = routes(&["192.168.1.20:47682", "192.168.64.1:47682", "[fd7a:115c:a1e0::1]:47682"]);

    assert_eq!(
        machine_code(&found, "c0de", Some("MacBook Pro de Miguel")).as_deref(),
        Some(
            "ws://192.168.1.20:47682/?code=c0de&also=192.168.64.1&also=fd7a:115c:a1e0::1&name=MacBook%20Pro%20de%20Miguel"
        )
    );
}

#[test]
fn an_ipv6_first_route_goes_in_brackets() {
    let url = machine_code(&routes(&["[fd7a:115c:a1e0::1]:47682"]), "c", None);

    assert_eq!(url.as_deref(), Some("ws://[fd7a:115c:a1e0::1]:47682/?code=c"));
}

#[test]
fn no_name_gives_no_name_and_no_route_gives_no_code() {
    assert_eq!(machine_code(&routes(&["10.0.0.1:1"]), "c", Some("")).as_deref(), Some("ws://10.0.0.1:1/?code=c"));
    assert_eq!(machine_code(&[], "c", Some("x")), None);
}

#[test]
fn a_long_name_is_cut_to_64_characters() {
    let url = machine_code(&routes(&["10.0.0.1:1"]), "c", Some(&"é".repeat(70))).expect("a code");

    assert!(url.ends_with(&format!("&name={}", "%C3%A9".repeat(64))));
}

#[test]
fn the_worst_case_still_fits_in_80_columns() {
    let found =
        routes(&["192.168.100.200:65535", "172.16.100.200:65535", "[fd7a:115c:a1e0:ab12:4843:cd96:6234:5678]:65535"]);
    let url = machine_code(&found, &"c".repeat(32), Some(&"x".repeat(70))).expect("a code");

    let width = code(&url).first().map_or(0, |line| line.spans.len());

    assert!((1..=80).contains(&width), "{width} columns");
}
