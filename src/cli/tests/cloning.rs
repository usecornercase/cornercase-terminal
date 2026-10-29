use std::path::Path;

use super::*;

fn devbox() -> Remote {
    Remote { destination: "devbox".into(), command: None, option: "--command" }
}

mod the_url {
    use super::*;

    #[test]
    fn of_a_repository_here_is_made_absolute_from_the_current_folder() {
        let here = std::env::current_dir().expect("the current folder");

        assert_eq!(clone_url("../repos/shop", None).expect("a url"), here.join("../repos/shop").display().to_string());
    }

    #[rstest]
    #[case::absolute("/srv/git/shop")]
    #[case::https("https://github.com/acme/shop.git")]
    #[case::scp_like("git@github.com:acme/shop.git")]
    #[case::home("~/src/shop")]
    fn that_is_not_a_relative_path_goes_as_it_is(#[case] url: &str) {
        assert_eq!(clone_url(url, None).expect("a url"), url);
    }

    #[test]
    fn of_another_machine_goes_as_it_is_for_its_server_to_refuse() {
        assert_eq!(clone_url("./shop", Some(&devbox())).expect("a url"), "./shop");
    }
}

mod the_folder {
    use super::*;

    #[test]
    fn is_the_current_one_by_default() {
        let here = std::env::current_dir().expect("the current folder");

        assert_eq!(clone_parent(None, None).expect("a folder"), here);
    }

    #[test]
    fn is_the_home_folder_of_another_machine_by_default() {
        assert_eq!(clone_parent(None, Some(&devbox())).expect("a folder"), PathBuf::from("~"));
    }

    #[test]
    fn of_another_machine_must_be_absolute() {
        let refused = clone_parent(Some(Path::new("src")), Some(&devbox())).expect_err("refused");

        assert!(refused.to_string().contains("give its absolute path on devbox"), "{refused}");
    }
}
