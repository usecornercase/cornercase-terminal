use super::*;

#[test]
fn lives_next_to_the_config() {
    assert_eq!(secrets(Path::new("/c/cornercase/config.json")), PathBuf::from("/c/cornercase/secrets.json"));
}
