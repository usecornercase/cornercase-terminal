use super::*;

#[rstest]
#[case::last_component("/home/ana/projects/cornercase", Some("/home/ana"), "cornercase")]
#[case::home_itself("/home/ana", Some("/home/ana"), "~")]
#[case::no_home("/tmp", None, "tmp")]
#[case::root("/", Some("/home/ana"), "/")]
fn shows_only_the_current_folder(#[case] path: &str, #[case] home: Option<&str>, #[case] expected: &str) {
    assert_eq!(folder_name(Path::new(path), home.map(Path::new)), expected);
}
