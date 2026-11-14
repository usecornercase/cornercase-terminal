use rstest::rstest;

use super::*;

#[rstest]
#[case::computer_first(Some("  Miguel's Mac "), "host.local", Some("Miguel's Mac"))]
#[case::host_without_local(None, "host.local\n", Some("host"))]
#[case::plain_host(None, "box", Some("box"))]
#[case::blank_computer_falls_back(Some("   "), "box", Some("box"))]
#[case::nothing(None, " ", None)]
#[case::only_local(None, ".local", None)]
fn machine_name_picks_what_to_show(#[case] computer: Option<&str>, #[case] host: &str, #[case] expected: Option<&str>) {
    assert_eq!(machine_name(computer.map(String::from), host).as_deref(), expected);
}
