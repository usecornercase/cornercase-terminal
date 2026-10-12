use std::os::unix::fs::MetadataExt;

use super::*;
use crate::test_util::TempDir;

mod pairing {
    use super::*;

    #[test]
    fn accepts_its_token_until_it_expires() {
        let now = Instant::now();
        let pairing = Pairing::new(now).expect("a token");

        assert!(pairing.accepts(&pairing.token.clone(), now));
        assert!(!pairing.accepts(&pairing.token.clone(), now + PAIRING_FOR));
    }

    #[test]
    fn refuses_another_token() {
        let now = Instant::now();
        let pairing = Pairing::new(now).expect("a token");

        assert!(!pairing.accepts("0".repeat(pairing.token.len()).as_str(), now));
        assert!(!pairing.accepts("", now));
    }

    #[test]
    fn tokens_differ_each_time() {
        let now = Instant::now();

        assert_ne!(Pairing::new(now).expect("one").token, Pairing::new(now).expect("two").token);
    }
}

mod devices {
    use super::*;

    #[test]
    fn a_paired_browser_is_known_by_its_cookie_after_a_reload() {
        let dir = TempDir::new();
        let file = dir.path().join("browsers.json");
        let mut devices = Devices::load(file.clone());

        let (id, cookie) = devices.add("Safari on iPhone").expect("paired");

        assert_eq!(Devices::load(file).check(&cookie), Some(id.as_str()));
    }

    #[test]
    fn the_file_keeps_only_a_hash_of_the_secret_and_only_its_owner_reads_it() {
        let dir = TempDir::new();
        let file = dir.path().join("browsers.json");
        let (_, cookie) = Devices::load(file.clone()).add("Safari").expect("paired");

        let text = std::fs::read_to_string(&file).expect("saved");
        let secret = cookie.split_once('.').expect("id and secret").1;

        assert!(!text.contains(secret));
        assert_eq!(std::fs::metadata(&file).expect("metadata").mode() & 0o777, 0o600);
    }

    #[test]
    fn a_wrong_or_malformed_cookie_is_refused() {
        let dir = TempDir::new();
        let mut devices = Devices::load(dir.path().join("browsers.json"));
        let (id, _) = devices.add("Safari").expect("paired");

        assert_eq!(devices.check(&format!("{id}.{}", "0".repeat(64))), None);
        assert_eq!(devices.check(&id), None);
        assert_eq!(devices.check(""), None);
    }

    #[test]
    fn clearing_forgets_every_browser() {
        let dir = TempDir::new();
        let file = dir.path().join("browsers.json");
        let mut devices = Devices::load(file.clone());
        let (_, cookie) = devices.add("Safari").expect("paired");

        devices.clear().expect("cleared");

        assert_eq!((devices.check(&cookie), Devices::load(file).len()), (None, 0));
    }

    #[test]
    fn a_corrupt_file_means_no_browsers() {
        let dir = TempDir::new();
        let file = dir.path().join("browsers.json");
        std::fs::write(&file, "{not json").expect("write");

        assert_eq!(Devices::load(file).len(), 0);
    }
}
