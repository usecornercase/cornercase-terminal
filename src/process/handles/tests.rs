use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::test_util::wait_until;

fn snapshot(entries: &[(usize, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(entries.len().to_ne_bytes());
    bytes.extend(0_usize.to_ne_bytes());
    for (handle, index) in entries {
        let mut entry = vec![0_u8; ENTRY];
        entry[..WORD].copy_from_slice(&handle.to_ne_bytes());
        entry[TYPE_INDEX..TYPE_INDEX + 4].copy_from_slice(&index.to_ne_bytes());
        bytes.extend(entry);
    }
    bytes
}

fn stuck(pid: i32) -> Vec<usize> {
    STUCK.lock().iter().filter(|(of, _)| *of == pid).map(|(_, handle)| *handle).collect()
}

fn file(handle: usize) -> PathBuf {
    PathBuf::from(format!("file-{handle}"))
}

fn even(handle: usize) -> Option<PathBuf> {
    handle.is_multiple_of(2).then(|| file(handle))
}

mod the_snapshot {
    use super::*;

    #[test]
    fn keeps_the_handles_of_one_type() {
        assert_eq!(of_type(&snapshot(&[(4, 37), (8, 12), (12, 37)]), 37), [4, 12]);
    }

    #[test]
    fn reads_no_more_entries_than_it_counts() {
        let mut bytes = snapshot(&[(4, 37), (8, 37)]);
        bytes[..WORD].copy_from_slice(&1_usize.to_ne_bytes());

        assert_eq!(of_type(&bytes, 37), [4]);
    }

    #[test]
    fn leaves_out_an_entry_cut_short() {
        let mut bytes = snapshot(&[(4, 37), (8, 37)]);
        bytes.pop();

        assert_eq!(of_type(&bytes, 37), [4]);
    }

    #[test]
    fn reads_nothing_from_a_cut_header() {
        assert_eq!(of_type(&snapshot(&[(4, 37)])[..WORD], 37), Vec::<usize>::new());
    }

    #[test]
    fn finds_the_type_of_one_handle() {
        let bytes = snapshot(&[(4, 37), (8, 12)]);

        assert_eq!((type_of(&bytes, 8), type_of(&bytes, 16)), (Some(12), None));
    }
}

mod naming {
    use super::*;

    #[test]
    fn names_every_handle_that_has_a_name() {
        let ask: Ask = Arc::new(even);

        assert_eq!(names(-1, vec![2, 3, 4], &ask), [file(2), file(4)]);
        assert_eq!(stuck(-1), Vec::<usize>::new());
    }

    #[test]
    fn skips_a_handle_whose_name_blocks_and_asks_again_once_it_answered() {
        let pid = -2;
        let (open, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let ask: Ask = Arc::new(move |handle| {
            if handle == 2 {
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = gate.lock().recv();
            }
            even(handle)
        });

        assert_eq!(names(pid, vec![2, 4, 6], &ask), [file(4), file(6)]);
        assert_eq!(stuck(pid), [2]);
        assert_eq!(names(pid, vec![2, 4], &ask), [file(4)]);
        assert_eq!(asked.load(Ordering::SeqCst), 1);

        drop(open);
        wait_until("the blocked name is answered", || stuck(pid).is_empty());

        assert_eq!(names(pid, vec![2], &ask), [file(2)]);
        assert_eq!(asked.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn goes_on_after_a_handle_whose_question_failed() {
        let ask: Ask = Arc::new(|handle| {
            assert_ne!(handle, 4, "the question about this handle fails");
            even(handle)
        });

        assert_eq!(names(-3, vec![2, 4, 6], &ask), [file(2), file(6)]);
        assert_eq!(stuck(-3), Vec::<usize>::new());
    }
}

#[cfg(windows)]
mod on_windows {
    use std::fs::{File, TryLockError};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;
    use crate::test_util::{Running, TempDir, exited_pid, holding, this_pid, wait_until_within};

    fn within(pid: i32) -> Vec<PathBuf> {
        let (answer, answered) = mpsc::channel();
        thread::spawn(move || answer.send(open_files(pid)));
        answered.recv_timeout(Duration::from_secs(5)).expect("the open files are read without hanging")
    }

    fn canonical(path: &Path) -> PathBuf {
        path.canonicalize().expect("canonical path")
    }

    #[test]
    fn names_a_file_this_process_holds_as_canonicalize_does() {
        let dir = TempDir::new();
        let path = dir.path().join("held.txt");
        let _held = File::create(&path).expect("create the file");

        assert!(within(this_pid()).contains(&canonical(&path)));
    }

    #[test]
    fn names_nothing_for_a_process_that_exited() {
        assert_eq!(within(exited_pid()), Vec::<PathBuf>::new());
    }

    fn locking(path: &Path) -> Running {
        let script = "$f = [IO.File]::Open($env:HELD, 'Open', 'ReadWrite', 'ReadWrite'); \
                      while ($true) { try { $f.Lock(0, 1); break } catch { Start-Sleep -Milliseconds 10 } }; \
                      [void][Console]::In.ReadLine()";
        Running::spawn(
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("HELD", path)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
    }

    fn locked(path: &Path) -> bool {
        let file = File::open(path).expect("open the locked file");
        matches!(file.try_lock(), Err(TryLockError::WouldBlock))
    }

    #[test]
    fn leaves_the_locks_of_the_process_it_reads() {
        let dir = TempDir::new();
        let path = dir.path().join("locked.db");
        File::create(&path).expect("create the file");
        let holder = locking(&path);
        wait_until_within("the other process locks the file", Duration::from_secs(30), || locked(&path));

        let named =
            within(holder.pid()).contains(&canonical(&path)) && within(holder.pid()).contains(&canonical(&path));

        assert!(named && locked(&path));
    }

    #[test]
    fn names_what_another_process_holds_while_a_read_blocks_one_of_its_handles() {
        let dir = TempDir::new();
        let path = dir.path().join("held.txt");
        let holder = holding(&path, &[]);
        let pid = holder.pid();

        wait_until("a handle blocked in a read is skipped", || {
            within(pid).contains(&canonical(&path)) && !stuck(pid).is_empty()
        });
        drop(holder);

        wait_until("the blocked question ends with the process", || stuck(pid).is_empty());
    }
}
