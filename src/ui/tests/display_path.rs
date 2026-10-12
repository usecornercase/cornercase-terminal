use super::*;

#[rstest]
#[case::under_home("/home/ana/.cornercase/w", "~/.cornercase/w")]
#[case::home_itself("/home/ana", "~")]
#[case::elsewhere("/srv/w", "/srv/w")]
fn shows_home_as_tilde(#[case] path: &str, #[case] expected: &str) {
    assert_eq!(display_path(Path::new(path), Some(Path::new("/home/ana"))), expected);
}
