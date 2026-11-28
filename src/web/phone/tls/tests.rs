use std::cell::Cell;
use std::path::PathBuf;

use super::*;
use crate::test_util::TempDir;
use crate::test_util::tls::sha256_base64url;

fn certificate_file(dir: &TempDir) -> PathBuf {
    dir.path().join("app-certificate.json")
}

fn saved(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("the file")).expect("json")
}

fn der(path: &Path) -> Vec<u8> {
    STANDARD.decode(saved(path)["certificate"].as_str().expect("a certificate")).expect("base64")
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|window| *window == needle).count()
}

mod loading {
    use super::*;

    #[test]
    fn a_missing_file_is_made_and_then_loaded_the_same() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        let made = Identity::load_or_make(&path, &|| Some("Mac".into())).expect("made");
        let asked = Cell::new(false);
        let loaded = Identity::load_or_make(&path, &|| {
            asked.set(true);
            None
        })
        .expect("loaded");

        assert_eq!(made.fingerprint, loaded.fingerprint);
        assert!(!asked.get(), "the name was asked for although nothing was made");
    }

    #[test]
    fn the_fingerprint_is_the_base64url_sha256_of_the_certificate() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        let identity = Identity::load_or_make(&path, &|| None).expect("made");

        assert_eq!(identity.fingerprint.len(), 43);
        assert!(identity.fingerprint.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(identity.fingerprint, sha256_base64url(&der(&path)));
    }

    #[test]
    fn the_file_holds_its_version_the_certificate_and_the_key() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        Identity::load_or_make(&path, &|| None).expect("made");

        let saved = saved(&path);
        assert_eq!(saved["version"], 1);
        assert!(saved["certificate"].is_string() && saved["key"].is_string());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        Identity::load_or_make(&path, &|| None).expect("made");

        let mode = std::fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_corrupt_file_is_replaced_with_another_certificate() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        let first = Identity::load_or_make(&path, &|| None).expect("made");
        let mut broken = saved(&path);
        broken["key"] = "bm90IGEga2V5".into();
        std::fs::write(&path, broken.to_string()).expect("write");

        let second = Identity::load_or_make(&path, &|| None).expect("made again");

        assert_ne!(first.fingerprint, second.fingerprint);
        assert_eq!(second.fingerprint, sha256_base64url(&der(&path)));
    }

    #[test]
    fn a_file_that_is_not_text_is_replaced_too() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        std::fs::write(&path, [0xff, 0xfe]).expect("write");

        let identity = Identity::load_or_make(&path, &|| None).expect("made");

        assert_eq!(identity.fingerprint, sha256_base64url(&der(&path)));
    }

    #[test]
    fn another_version_is_replaced_too() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        let first = Identity::load_or_make(&path, &|| None).expect("made");
        let mut other = saved(&path);
        other["version"] = 2.into();
        std::fs::write(&path, other.to_string()).expect("write");

        let second = Identity::load_or_make(&path, &|| None).expect("made again");

        assert_ne!(first.fingerprint, second.fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_it_may_not_read_is_an_error_and_is_left_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        Identity::load_or_make(&path, &|| None).expect("made");
        let before = der(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("no access");

        let loaded = Identity::load_or_make(&path, &|| None);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("access again");
        assert!(loaded.is_err());
        assert_eq!(der(&path), before);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_an_error_and_is_left_alone() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        std::fs::create_dir(&path).expect("a folder in its place");

        let loaded = Identity::load_or_make(&path, &|| None);

        assert!(loaded.is_err());
        assert!(path.is_dir());
    }
}

mod making {
    use super::*;

    #[test]
    fn making_over_a_certificate_replaces_it() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);
        let first = Identity::load_or_make(&path, &|| None).expect("made");

        let second = Identity::make(&path, None).expect("replaced");
        let loaded = Identity::load_or_make(&path, &|| None).expect("loaded");

        assert_ne!(first.fingerprint, second.fingerprint);
        assert_eq!(second.fingerprint, loaded.fingerprint);
    }

    #[test]
    fn the_subject_holds_the_computer_s_name() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        Identity::make(&path, Some("MacBook de Miguel")).expect("made");

        let der = der(&path);
        assert_eq!(count(&der, b"MacBook de Miguel"), 2, "the subject and, self-signed, the issuer");
        assert_eq!(count(&der, DNS_NAME.as_bytes()), 1);
    }

    #[test]
    fn no_name_gives_cornercase() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        Identity::make(&path, Some("  ")).expect("made");

        assert_eq!(count(&der(&path), DNS_NAME.as_bytes()), 3, "the subject, the issuer and the DNS name");
    }

    #[test]
    fn a_long_name_is_cut_to_64_characters() {
        let dir = TempDir::new();
        let path = certificate_file(&dir);

        Identity::make(&path, Some(&"x".repeat(70))).expect("made");

        let der = der(&path);
        assert_eq!((count(&der, "x".repeat(64).as_bytes()), count(&der, "x".repeat(65).as_bytes())), (2, 0));
    }
}
