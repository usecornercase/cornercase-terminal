use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, wait_until, write_executable};

fn cue(kind: Kind, path: &str) -> Cue {
    Cue { kind, path: path.into() }
}

mod builtin_sounds {
    use super::*;

    #[rstest]
    #[case::needs_you(Kind::NeedsYou)]
    #[case::finished(Kind::Finished)]
    fn are_short_pcm_wave_files(#[case] kind: Kind) {
        let bytes = wav(kind);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16, "16-bit");
        assert_eq!(&bytes[36..40], b"data");
        let riff = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(riff + 8, bytes.len());
        let data = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert!(f64::from(data) / f64::from(u32::from(RATE) * 2) <= 0.4, "at most 0.4 s");
    }

    #[test]
    fn the_two_differ() {
        assert_ne!(wav(Kind::NeedsYou), wav(Kind::Finished));
    }

    #[test]
    fn are_written_once_in_a_private_folder() {
        let tmp = TempDir::new();
        let path = builtin(tmp.path(), Kind::NeedsYou).expect("write");
        assert_eq!(std::fs::read(&path).expect("read"), wav(Kind::NeedsYou));
        let mode = |p: &Path| p.metadata().expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().expect("folder")), 0o700);
        let written = path.metadata().expect("metadata").modified().expect("time");
        assert_eq!(builtin(tmp.path(), Kind::NeedsYou).expect("again"), path);
        assert_eq!(path.metadata().expect("metadata").modified().expect("time"), written);
    }

    #[test]
    fn are_written_again_when_their_bytes_differ() {
        let tmp = TempDir::new();
        let path = builtin(tmp.path(), Kind::Finished).expect("write");
        let old = vec![0; wav(Kind::Finished).len()];
        std::fs::write(&path, &old).expect("an older sound of the same size");
        assert_eq!(builtin(tmp.path(), Kind::Finished).expect("again"), path);
        assert_eq!(std::fs::read(&path).expect("read"), wav(Kind::Finished));
    }
}

mod choosing_a_player {
    use super::*;

    fn fake_path(names: &[&str]) -> TempDir {
        let tmp = TempDir::new();
        for name in names {
            write_executable(&tmp.path().join(name), "#!/bin/sh\n");
        }
        tmp
    }

    #[rstest]
    #[case::macos(Os::Macos, &["afplay", "aplay"], Some("afplay"))]
    #[case::macos_without_afplay(Os::Macos, &["aplay"], None)]
    #[case::pipewire_first(Os::Linux, &["aplay", "paplay", "pw-play"], Some("pw-play"))]
    #[case::pulseaudio_next(Os::Linux, &["aplay", "paplay"], Some("paplay"))]
    #[case::alsa_last(Os::Linux, &["aplay"], Some("aplay"))]
    #[case::linux_without_one(Os::Linux, &["afplay"], None)]
    fn takes_the_first_one_on_the_path(#[case] os: Os, #[case] names: &[&str], #[case] expected: Option<&str>) {
        let tmp = fake_path(names);
        let path = tmp.path().as_os_str().to_owned();
        let found = player(os, on_path(Some(&path)));
        assert_eq!(found, expected.map(|name| tmp.path().join(name)));
    }

    #[test]
    fn skips_files_that_are_not_executable() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join("afplay"), "").expect("write");
        let path = tmp.path().as_os_str().to_owned();
        assert_eq!(player(Os::Macos, on_path(Some(&path))), None);
    }

    #[test]
    fn finds_none_without_a_path() {
        assert_eq!(player(Os::Linux, on_path(None)), None);
    }
}

mod resolving_a_path {
    use super::*;

    #[test]
    fn expands_the_home_folder() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join("ding.wav"), "x").expect("write");
        assert_eq!(resolve("~/ding.wav", Some(tmp.path())), Some(tmp.path().join("ding.wav")));
    }

    #[rstest]
    #[case::empty("")]
    #[case::blank("  ")]
    #[case::missing("~/nothing.wav")]
    #[case::folder("~")]
    #[case::relative_to_the_window("Cargo.toml")]
    #[case::like_an_option("-q")]
    fn falls_back_to_the_builtin(#[case] path: &str) {
        let tmp = TempDir::new();
        assert_eq!(resolve(path, Some(tmp.path())), None);
    }
}

mod coalescing {
    use super::*;

    #[test]
    fn needs_you_wins() {
        let cues = [cue(Kind::Finished, "a"), cue(Kind::NeedsYou, "b"), cue(Kind::NeedsYou, "c")];
        assert_eq!(loudest(&cues), Some(cue(Kind::NeedsYou, "b")));
    }

    #[test]
    fn keeps_the_first_of_one_kind() {
        let cues = [cue(Kind::Finished, "a"), cue(Kind::Finished, "b")];
        assert_eq!(loudest(&cues), Some(cue(Kind::Finished, "a")));
    }

    #[test]
    fn nothing_from_nothing() {
        assert_eq!(loudest(&[]), None);
    }
}

mod on_this_machine {
    use super::*;

    #[rstest]
    #[case::plain(&[], true)]
    #[case::ssh_connection(&[("SSH_CONNECTION", "10.0.0.2 51000 10.0.0.1 22")], false)]
    #[case::ssh_tty(&[("SSH_TTY", "/dev/pts/3")], false)]
    #[case::empty_variable(&[("SSH_CONNECTION", "")], true)]
    fn is_false_under_ssh(#[case] vars: &[(&str, &str)], #[case] expected: bool) {
        let var = |name: &str| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_string());
        assert_eq!(local_machine(var), expected);
    }
}

mod playing {
    use super::*;

    fn recording_player(tmp: &TempDir) -> PathBuf {
        let program = tmp.path().join("afplay");
        let log = tmp.path().join("played");
        write_executable(&program, &format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display()));
        program
    }

    fn played(tmp: &TempDir) -> String {
        std::fs::read_to_string(tmp.path().join("played")).unwrap_or_default()
    }

    #[test]
    fn plays_the_builtin_through_the_player() {
        let tmp = TempDir::new();
        let mut player = Player::new(Some(recording_player(&tmp)), tmp.path().into(), None);
        assert!(player.play(&cue(Kind::Finished, ""), Instant::now()));
        let expected = tmp.path().join("sounds").join("finished.wav");
        wait_until("the player ran", || played(&tmp).trim() == expected.display().to_string());
    }

    #[test]
    fn plays_a_custom_file() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join("ding.wav"), "x").expect("write");
        let mut player = Player::new(Some(recording_player(&tmp)), tmp.path().into(), Some(tmp.path().into()));
        assert!(player.play(&cue(Kind::NeedsYou, "~/ding.wav"), Instant::now()));
        let expected = tmp.path().join("ding.wav");
        wait_until("the player ran", || played(&tmp).trim() == expected.display().to_string());
    }

    #[test]
    fn drops_a_sound_within_a_second_of_the_last() {
        let tmp = TempDir::new();
        let mut player = Player::new(Some(recording_player(&tmp)), tmp.path().into(), None);
        let now = Instant::now();
        assert!(player.play(&cue(Kind::Finished, ""), now));
        assert!(!player.play(&cue(Kind::NeedsYou, ""), now + Duration::from_millis(999)));
        assert!(player.play(&cue(Kind::NeedsYou, ""), now + GAP));
        wait_until("both ran", || played(&tmp).lines().count() == 2);
    }

    #[test]
    fn does_nothing_without_a_player() {
        let tmp = TempDir::new();
        let mut player = Player::new(None, tmp.path().into(), None);
        assert!(!player.play(&cue(Kind::NeedsYou, ""), Instant::now()));
        assert!(!tmp.path().join("sounds").exists());
    }

    #[test]
    fn a_player_that_cannot_start_only_waits_for_the_next_sound() {
        let tmp = TempDir::new();
        let mut player = Player::new(Some(tmp.path().join("missing")), tmp.path().into(), None);
        let now = Instant::now();
        assert!(player.play(&cue(Kind::NeedsYou, ""), now));
        player.program = Some(recording_player(&tmp));
        assert!(player.play(&cue(Kind::NeedsYou, ""), now + GAP));
        wait_until("the second player ran", || played(&tmp).lines().count() == 1);
    }

    #[test]
    fn passes_quiet_to_aplay() {
        let tmp = TempDir::new();
        let program = tmp.path().join("aplay");
        let log = tmp.path().join("played");
        write_executable(&program, &format!("#!/bin/sh\necho \"$@\" >> '{}'\n", log.display()));
        let mut player = Player::new(Some(program), tmp.path().into(), None);
        assert!(player.play(&cue(Kind::NeedsYou, ""), Instant::now()));
        wait_until("aplay ran", || played(&tmp).starts_with("-q "));
    }
}
