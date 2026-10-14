use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::*;
use crate::test_util::{TempDir, write_executable};

fn config(sound: &str, waiting: &str, done: &str) -> Config {
    Config { sound: sound.into(), sound_waiting: waiting.into(), sound_done: done.into(), ..Config::default() }
}

fn builtin(kind: Kind) -> Request {
    Request { kind, file: None }
}

mod setting {
    use super::*;

    #[test]
    fn stays_quiet_until_sound_is_on() {
        assert_eq!(cue(true, &config("off", "", "")), None);
        assert_eq!(cue(true, &config("no", "", "")), None);
    }

    #[test]
    fn an_agent_that_needs_you_and_one_that_finishes_use_different_sounds() {
        let on = config("ON", "builtin", "~/done.wav");

        assert_eq!(cue(true, &on), Some(Cue { kind: Kind::Waiting, path: String::new() }));
        assert_eq!(cue(false, &on), Some(Cue { kind: Kind::Done, path: "~/done.wav".into() }));
    }

    #[test]
    fn a_path_has_to_be_absolute() {
        let home = Path::new("/home/ana");

        assert_eq!(check_path("  ", Some(home)), Ok(String::new()));
        assert_eq!(check_path("builtin", Some(home)), Ok(String::new()));
        assert_eq!(check_path("~/sounds/done.wav", Some(home)), Ok("~/sounds/done.wav".into()));
        assert_eq!(check_path("/tmp/done.wav", None), Ok("/tmp/done.wav".into()));
        assert_eq!(check_path("done.wav", Some(home)), Err("use an absolute path or one starting with ~/"));
        assert_eq!(check_path("/tmp/a\nb.wav", Some(home)), Err("the path cannot contain control characters"));
    }

    #[test]
    fn the_row_shows_built_in_or_the_path() {
        assert_eq!(
            (label(""), label("builtin"), label("~/a.wav")),
            ("built-in".into(), "built-in".into(), "~/a.wav".into())
        );
        assert_eq!((picked(""), picked("/tmp/a.wav")), (BUILTIN, FILE));
        assert_eq!(switch_label(" ON "), ON);
    }
}

mod tone {
    use super::*;

    #[test]
    fn the_two_built_in_sounds_are_short_wavs_and_not_the_same() {
        let waiting = wav(Kind::Waiting);
        let done = wav(Kind::Done);

        assert!(waiting.starts_with(b"RIFF"));
        assert!(done.starts_with(b"RIFF"));
        assert_ne!(waiting, done);
        for bytes in [&waiting, &done] {
            let samples = (bytes.len() - 44) / 2;
            assert!(samples > 0 && samples < usize::try_from(RATE).expect("rate") / 2, "{}", bytes.len());
            assert!(bytes[44..].iter().any(|byte| *byte != 0));
        }
    }
}

mod player {
    use super::*;

    #[test]
    fn the_first_player_that_exists_plays_the_built_in_sound() {
        let tmp = TempDir::new();
        let played = tmp.path().join("played");
        let bin = tmp.path().join("player");
        write_executable(&bin, &format!("#!/bin/sh\ncp \"$1\" '{}'\n", played.display()));
        let seen = Mutex::new(Vec::new());
        let names = player_names();
        let prepared = prepare(&builtin(Kind::Waiting), |name| {
            seen.lock().expect("seen").push(name.to_string());
            (name == names[0]).then(|| bin.clone())
        })
        .expect("a player");

        assert_eq!(seen.lock().expect("seen").as_slice(), &[names[0].to_string()]);
        assert!(prepared.temp);
        let mut child = spawn(&prepared).expect("spawn");
        assert!(child.wait().expect("wait").success());
        assert!(fs::read(&played).expect("recorded").starts_with(b"RIFF"));
        fs::remove_file(&prepared.file).expect("remove the temporary wav");
    }

    #[test]
    fn a_later_player_is_used_when_the_earlier_ones_are_missing() {
        let seen = Mutex::new(Vec::new());
        let names = player_names();
        let prepared = prepare(&builtin(Kind::Done), |name| {
            seen.lock().expect("seen").push(name.to_string());
            (name == *names.last().expect("a name")).then(|| PathBuf::from(name))
        })
        .expect("a player");

        assert_eq!(prepared.player, PathBuf::from(names.last().expect("a name")));
        assert_eq!(
            seen.lock().expect("seen").as_slice(),
            &names.iter().map(|name| (*name).to_string()).collect::<Vec<_>>()
        );
        fs::remove_file(&prepared.file).expect("remove the temporary wav");
    }

    #[test]
    fn a_missing_player_or_a_missing_file_plays_nothing() {
        assert!(prepare(&builtin(Kind::Waiting), |_| None).is_none());
        let missing = Request { kind: Kind::Done, file: Some(PathBuf::from("/no/such/cornercase-sound.wav")) };
        assert!(prepare(&missing, |_| Some(PathBuf::from("paplay"))).is_none());
    }

    #[test]
    fn a_file_of_the_users_is_played_and_kept() {
        let tmp = TempDir::new();
        let wav = tmp.path().join("done.wav");
        fs::write(&wav, b"RIFF").expect("write");
        let request = Request { kind: Kind::Done, file: Some(wav.clone()) };

        let prepared = prepare(&request, |_| Some(PathBuf::from("afplay"))).expect("a player");

        assert_eq!((prepared.file, prepared.temp), (wav, false));
    }
}

fn spawn(prepared: &Prepared) -> std::io::Result<Child> {
    Command::new(&prepared.player)
        .arg(&prepared.file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}
