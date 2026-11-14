use super::*;

#[test]
fn a_temp_folder_goes_once_its_guard_is_dropped() {
    let folder = temp_dir("guarded");
    std::fs::write(folder.join("file"), "kept while the guard lives").expect("write a file");
    let path = folder.to_path_buf();
    assert_eq!(path.parent(), Some(temp().as_path()), "{}", path.display());
    drop(folder);
    assert!(!path.exists(), "{} is still there", path.display());
}

#[test]
fn a_temp_folder_goes_when_its_test_panics() {
    let mut made = None;
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let folder = temp_dir_named(&format!("ccguard-{}", std::process::id()));
        std::fs::create_dir(folder.join("inside")).expect("fill the folder");
        made = Some(folder.to_path_buf());
        panic!("the test fails while its folder is in use");
    }));
    assert!(unwound.is_err());
    let made = made.expect("the folder was made");
    assert!(!made.exists(), "{} is still there", made.display());
}

#[test]
fn a_folder_of_the_session_goes_with_it() {
    let session = Session::new();
    let folder = session.folder("project");
    std::fs::write(folder.join("file"), "x").expect("write a file");
    assert_eq!(folder, folder.canonicalize().expect("canonical"));
    drop(session);
    assert!(!folder.exists(), "{} is still there", folder.display());
}
