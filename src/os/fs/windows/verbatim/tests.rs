use rstest::rstest;

use super::*;

fn verbatim_of_length(units: usize) -> String {
    let first = "a".repeat(120);
    let rest = units - VERBATIM.len() - r"C:\".len() - first.len() - 1;
    let verbatim = format!(r"{VERBATIM}C:\{first}\{}", "b".repeat(rest));
    assert_eq!(verbatim.len(), units);
    verbatim
}

#[rstest]
#[case::a_folder(r"\\?\C:\Users\me\repo", r"C:\Users\me\repo")]
#[case::a_drive_s_root(r"\\?\C:\", r"C:\")]
#[case::a_lower_case_drive(r"\\?\d:\work", r"d:\work")]
#[case::names_out_of_ascii(r"\\?\C:\Users\José\año\ñandú", r"C:\Users\José\año\ñandú")]
#[case::names_with_spaces_and_dots_inside(r"\\?\C:\my repo\v1.2 final", r"C:\my repo\v1.2 final")]
#[case::names_that_only_start_like_a_device(r"\\?\C:\console\auxiliary\com10\lpt", r"C:\console\auxiliary\com10\lpt")]
fn a_drive_path_loses_the_prefix(#[case] path: &str, #[case] plain: &str) {
    assert_eq!(simplified(PathBuf::from(path)).as_os_str(), plain);
}

#[rstest]
#[case::a_share(r"\\?\UNC\server\share\repo")]
#[case::a_volume(r"\\?\Volume{0b1c2d3e-0000-0000-0000-100000000000}\repo")]
#[case::a_drive_without_its_root(r"\\?\C:")]
#[case::a_parent(r"\\?\C:\repo\..\other")]
#[case::a_current_folder(r"\\?\C:\repo\.\src")]
#[case::a_slash_inside_a_name(r"\\?\C:\repo/src")]
#[case::a_colon_inside_a_name(r"\\?\C:\repo\file:stream")]
#[case::a_forbidden_character(r"\\?\C:\repo\what?")]
#[case::a_control_character("\\\\?\\C:\\repo\\a\u{1}b")]
#[case::a_trailing_dot(r"\\?\C:\repo.")]
#[case::a_trailing_space(r"\\?\C:\repo ")]
#[case::a_device(r"\\?\C:\repo\CON")]
#[case::a_device_in_lower_case(r"\\?\C:\repo\nul")]
#[case::a_device_with_an_extension(r"\\?\C:\repo\aux.txt")]
#[case::a_device_with_spaces_before_its_extension(r"\\?\C:\repo\prn  .tar.gz")]
#[case::the_console_s_input(r"\\?\C:\repo\CONIN$")]
#[case::a_serial_port(r"\\?\C:\repo\COM1")]
#[case::a_printer_port(r"\\?\C:\repo\lpt9.log")]
#[case::a_port_with_a_superscript(r"\\?\C:\repo\COM²")]
#[case::a_port_numbered_zero(r"\\?\C:\repo\LPT0")]
fn a_path_win32_would_read_otherwise_keeps_the_prefix(#[case] path: &str) {
    assert_eq!(simplified(PathBuf::from(path)).as_os_str(), path);
}

#[test]
fn a_path_keeps_the_prefix_once_it_is_over_max_path_with_it() {
    let longest = verbatim_of_length(MAX_PATH);
    assert_eq!(simplified(PathBuf::from(&longest)).as_os_str(), &longest[VERBATIM.len()..]);
    let too_long = verbatim_of_length(MAX_PATH + 1);
    assert_eq!(simplified(PathBuf::from(&too_long)).as_os_str(), too_long.as_str());
}

#[test]
fn the_length_counts_utf_16_units_not_bytes() {
    let path = format!(r"{VERBATIM}C:\{}", "ñ".repeat(200));
    assert!(path.len() > MAX_PATH);
    assert_eq!(simplified(PathBuf::from(&path)).as_os_str(), &path[VERBATIM.len()..]);
}

#[test]
fn a_name_longer_than_a_file_system_takes_keeps_the_prefix() {
    let path = format!(r"{VERBATIM}C:\{}", "a".repeat(MAX_NAME + 1));
    assert_eq!(simplified(PathBuf::from(&path)).as_os_str(), path.as_str());
}

#[rstest]
#[case::a_plain_drive_path(r"C:\Users\me\repo")]
#[case::a_plain_share(r"\\server\share\repo")]
#[case::a_device_path(r"\\.\pipe\cornercase")]
#[case::a_relative_path(r"src\main.rs")]
fn a_path_without_the_prefix_stays_as_it_is(#[case] path: &str) {
    assert_eq!(simplified(PathBuf::from(path)).as_os_str(), path);
}
