use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;

use super::*;
use crate::test_util::wait_until;

const PATIENT: Duration = Duration::from_secs(30);
const BLOCKED_AFTER: Duration = Duration::from_secs(2);
const ONLY_BLOCKED_AFTER: Duration = Duration::from_millis(100);

type Ask = Arc<dyn Fn(usize) -> Option<PathBuf> + Send + Sync>;

struct Fake {
    object: usize,
    ask: Ask,
}

impl Object for Fake {
    fn name(&self) -> Option<PathBuf> {
        (self.ask)(self.object)
    }

    fn same(&self, other: &dyn Object) -> bool {
        self.object == other.raw()
    }

    fn raw(&self) -> usize {
        self.object
    }
}

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

fn waiting(most: usize) -> &'static Waiting {
    Box::leak(Box::new(Waiting::new(most)))
}

fn waiting_on(waiting: &Waiting, pid: i32) -> Vec<usize> {
    waiting.asked.lock().iter().filter(|question| question.key.0 == pid).map(|question| question.key.1).collect()
}

fn holders(waiting: &Waiting, key: (i32, usize)) -> usize {
    let asked = waiting.asked.lock();
    asked.iter().find(|question| question.key == key).map_or(0, |question| Arc::strong_count(&question.answer))
}

fn opening(ask: &Ask, object: impl Fn(usize) -> usize + Send + Sync + 'static) -> Open {
    let ask = Arc::clone(ask);
    Arc::new(move |handle| Some(Arc::new(Fake { object: object(handle), ask: Arc::clone(&ask) }) as Arc<dyn Object>))
}

fn asking(ask: &Ask, within: Duration, waiting: &'static Waiting) -> Asking {
    Asking { open: opening(ask, |handle| handle), within, waiting }
}

fn file(handle: usize) -> PathBuf {
    PathBuf::from(format!("file-{handle}"))
}

fn even(handle: usize) -> Option<PathBuf> {
    handle.is_multiple_of(2).then(|| file(handle))
}

struct Gated {
    ask: Ask,
    blocked: Arc<AtomicUsize>,
    open: Option<mpsc::Sender<()>>,
}

impl Gated {
    fn new(blocks: impl Fn(usize) -> bool + Send + Sync + 'static) -> Self {
        let (open, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let blocked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&blocked);
        let ask: Ask = Arc::new(move |object| {
            if blocks(object) {
                counted.fetch_add(1, Ordering::SeqCst);
                let _ = gate.lock().recv();
            }
            even(object)
        });
        Self { ask, blocked, open: Some(open) }
    }

    fn blocked(&self) -> usize {
        self.blocked.load(Ordering::SeqCst)
    }

    fn open(&mut self) {
        self.open = None;
    }
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
        let waiting = waiting(16);
        let ask: Ask = Arc::new(even);

        assert_eq!(names(1, vec![2, 3, 4], &asking(&ask, PATIENT, waiting)), [file(2), file(4)]);
        assert_eq!(waiting_on(waiting, 1), Vec::<usize>::new());
    }

    #[test]
    fn skips_a_handle_whose_name_blocks_and_asks_again_once_it_answered() {
        let waiting = waiting(16);
        let mut gated = Gated::new(|object| object == 2);

        assert_eq!(names(1, vec![2, 4, 6], &asking(&gated.ask, BLOCKED_AFTER, waiting)), [file(4), file(6)]);
        assert_eq!(waiting_on(waiting, 1), [2]);
        assert_eq!(names(1, vec![2, 4], &asking(&gated.ask, BLOCKED_AFTER, waiting)), [file(4)]);
        assert_eq!(gated.blocked(), 1);

        gated.open();
        wait_until("the blocked name is answered", || waiting_on(waiting, 1).is_empty());

        assert_eq!(names(1, vec![2], &asking(&gated.ask, PATIENT, waiting)), [file(2)]);
        assert_eq!(gated.blocked(), 2);
    }

    #[test]
    fn waits_for_the_name_another_call_is_asking_and_takes_its_answer() {
        let waiting = waiting(16);
        let mut gated = Gated::new(|object| object == 2);
        let first = asking(&gated.ask, PATIENT, waiting);
        let earlier = thread::spawn(move || names(1, vec![2], &first));
        wait_until("the first call asks", || gated.blocked() == 1);
        let second = asking(&gated.ask, PATIENT, waiting);
        let later = thread::spawn(move || names(1, vec![2, 4], &second));
        wait_until("the second call waits for that answer", || holders(waiting, (1, 2)) == 3);

        gated.open();

        assert_eq!(later.join().expect("the second call ends"), [file(2), file(4)]);
        assert_eq!(earlier.join().expect("the first call ends"), [file(2)]);
        assert_eq!((gated.blocked(), waiting_on(waiting, 1)), (1, vec![]));
    }

    #[test]
    fn asks_a_handle_value_that_now_names_another_file() {
        let waiting = waiting(16);
        let mut gated = Gated::new(|object| object == 2);
        let reused = Arc::new(AtomicUsize::new(0));
        let next = Arc::clone(&reused);
        let open = opening(&gated.ask, move |handle| handle + 100 * next.load(Ordering::SeqCst));
        let first = Asking { open, within: ONLY_BLOCKED_AFTER, waiting };
        assert_eq!(names(1, vec![2], &first), Vec::<PathBuf>::new());
        reused.store(1, Ordering::SeqCst);

        let again = names(1, vec![2], &Asking { within: BLOCKED_AFTER, ..first });

        assert_eq!((again, gated.blocked(), waiting_on(waiting, 1)), (vec![file(102)], 1, vec![2]));
        gated.open();
        wait_until("the blocked name is answered", || waiting_on(waiting, 1).is_empty());
    }

    #[test]
    fn asks_no_more_names_at_once_than_the_most_and_says_so_once() {
        let waiting = waiting(2);
        let mut gated = Gated::new(|_| true);
        let first = asking(&gated.ask, PATIENT, waiting);
        let earlier = thread::spawn(move || names(1, vec![1], &first));
        wait_until("the first call asks", || gated.blocked() == 1);
        assert_eq!(names(2, vec![1], &asking(&gated.ask, ONLY_BLOCKED_AFTER, waiting)), Vec::<PathBuf>::new());
        assert!(!waiting.full.load(Ordering::Relaxed));

        let full = names(3, vec![2, 4], &asking(&gated.ask, PATIENT, waiting));
        let again = names(4, vec![2], &asking(&gated.ask, PATIENT, waiting));

        assert_eq!((full, again, gated.blocked()), (vec![], vec![], 2));
        assert!(!waiting.filled(), "the first full call warned, and only it");
        gated.open();
        assert_eq!(earlier.join().expect("the first call ends"), Vec::<PathBuf>::new());
        wait_until("every name is answered", || waiting.asked.lock().is_empty());
        assert_eq!(names(3, vec![2, 4], &asking(&gated.ask, PATIENT, waiting)), [file(2), file(4)]);
    }

    #[test]
    fn warns_only_the_first_time_the_list_is_full() {
        let waiting = waiting(1);

        assert_eq!((waiting.filled(), waiting.filled()), (true, false));
    }

    #[test]
    fn goes_on_after_a_handle_whose_question_failed() {
        let waiting = waiting(16);
        let ask: Ask = Arc::new(|handle| {
            assert_ne!(handle, 4, "the question about this handle fails");
            even(handle)
        });

        assert_eq!(names(1, vec![2, 4, 6], &asking(&ask, PATIENT, waiting)), [file(2), file(6)]);
        assert_eq!(waiting_on(waiting, 1), Vec::<usize>::new());
    }
}

#[cfg(windows)]
mod on_windows {
    use std::fs::{File, TryLockError};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::process::{Command, Stdio};

    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    use super::*;
    use crate::test_util::{Running, TempDir, exited_pid, holding, this_pid, wait_until_within};

    fn within(pid: i32) -> Vec<PathBuf> {
        let (answer, answered) = mpsc::channel();
        thread::spawn(move || answer.send(open_files(pid)));
        answered.recv_timeout(PATIENT).expect("the open files are read without hanging")
    }

    fn canonical(path: &Path) -> PathBuf {
        path.canonicalize().expect("canonical path")
    }

    fn named(pid: i32, path: &Path) {
        wait_until_within("the file is named", PATIENT, || within(pid).contains(&canonical(path)));
    }

    #[test]
    fn names_a_file_this_process_holds_as_canonicalize_does() {
        let dir = TempDir::new();
        let path = dir.path().join("held.txt");
        let _held = File::create(&path).expect("create the file");

        named(this_pid(), &path);
    }

    #[test]
    fn names_a_file_whose_path_is_longer_than_the_first_buffer() {
        let dir = TempDir::new();
        let folder =
            (0..6).fold(dir.path().to_path_buf(), |path, depth| path.join(format!("{depth}{}", "d".repeat(110))));
        std::fs::create_dir_all(&folder).expect("create the nested folders");
        let path = folder.join("held.txt");
        let _held = File::create(&path).expect("create the file");
        assert!(canonical(&path).as_os_str().encode_wide().count() > 512, "the path outgrows the first buffer");

        named(this_pid(), &path);
    }

    #[test]
    fn tells_two_copies_of_one_file_from_a_copy_of_another() {
        let dir = TempDir::new();
        let one = File::create(dir.path().join("one")).expect("create a file");
        let other = File::create(dir.path().join("other")).expect("create another file");
        let here = unsafe { GetCurrentProcess() };
        let copy = |file: &File| imp::duplicate(here, file.as_raw_handle().addr()).expect("a copy of the handle");

        let (first, second, third) = (copy(&one), copy(&one), copy(&other));

        assert_eq!((first.same(&second), first.same(&third)), (true, false));
    }

    #[test]
    fn names_nothing_for_a_process_that_exited() {
        assert_eq!(within(exited_pid()), Vec::<PathBuf>::new());
    }

    fn locking(path: &Path) -> Running {
        let script = "$f = [IO.File]::Open($env:HELD, 'Open', 'ReadWrite', 'ReadWrite'); $f.Lock(0, 1); \
                      [Console]::Out.WriteLine('locked'); [Console]::Out.Flush(); [void][Console]::In.ReadLine()";
        let mut holder = Running::spawn(
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("HELD", path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null()),
        );
        assert_eq!(holder.first_line(), "locked", "the other process says it holds the lock");
        holder
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
        assert!(locked(&path));

        named(holder.pid(), &path);
        named(holder.pid(), &path);

        assert!(locked(&path));
    }

    #[test]
    fn names_what_another_process_holds_while_a_read_blocks_one_of_its_handles() {
        let dir = TempDir::new();
        let path = dir.path().join("held.txt");
        let holder = holding(&path, &[]);
        let pid = holder.pid();

        wait_until_within("a handle blocked in a read is skipped", PATIENT, || {
            within(pid).contains(&canonical(&path)) && !waiting_on(&WAITING, pid).is_empty()
        });
        drop(holder);

        wait_until_within("the blocked question ends with the process", PATIENT, || {
            waiting_on(&WAITING, pid).is_empty()
        });
    }
}
