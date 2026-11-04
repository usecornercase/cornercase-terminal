use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::{log, panics};

#[cfg(windows)]
pub(crate) use imp::{hand_down_nothing, open_files};

#[cfg(windows)]
const NAME_WITHIN: Duration = Duration::from_secs(1);
#[cfg(windows)]
const MOST_ASKED: usize = 16;
const WORD: usize = size_of::<usize>();
const HEADER: usize = 2 * WORD;
const TYPE_INDEX: usize = 3 * WORD + 4;
const ENTRY: usize = 3 * WORD + 16;
const ATTRIBUTES: usize = 3 * WORD + 8;
const INHERIT: u32 = 2;

#[cfg(windows)]
static WAITING: Waiting = Waiting::new(MOST_ASKED);

trait Object: Send + Sync {
    fn name(&self) -> Option<PathBuf>;
    fn same(&self, other: &dyn Object) -> bool;
    fn raw(&self) -> usize;
}

type Open = Arc<dyn Fn(usize) -> Option<Arc<dyn Object>> + Send + Sync>;

#[derive(Clone)]
struct Asking {
    open: Open,
    within: Duration,
    waiting: &'static Waiting,
}

struct Waiting {
    asked: Mutex<Vec<Question>>,
    most: usize,
    full: AtomicBool,
}

struct Question {
    object: Arc<dyn Object>,
    since: Instant,
    answer: Arc<Slot>,
}

#[derive(Default)]
struct Slot {
    told: Mutex<Told>,
    given: Condvar,
}

#[derive(Default)]
enum Told {
    #[default]
    Waiting,
    Named(Option<PathBuf>),
}

#[derive(Default)]
struct Progress {
    done: AtomicUsize,
    abandoned: AtomicBool,
}

enum Turn {
    Ask(Arc<Slot>),
    Wait(Arc<Slot>, Instant),
    Skip,
    Full,
    Stop,
}

enum Answer {
    Named(Option<PathBuf>),
    Full,
}

impl Progress {
    fn pass(&self) -> bool {
        self.done.fetch_add(1, Ordering::Relaxed);
        !self.abandoned.load(Ordering::Relaxed)
    }
}

impl Slot {
    fn give(&self, path: Option<PathBuf>) {
        *self.told.lock() = Told::Named(path);
        self.given.notify_all();
    }

    fn wait(&self, until: Instant) -> Option<PathBuf> {
        let mut told = self.told.lock();
        while matches!(*told, Told::Waiting) && !self.given.wait_until(&mut told, until).timed_out() {}
        match &*told {
            Told::Named(path) => path.clone(),
            Told::Waiting => None,
        }
    }
}

impl Waiting {
    const fn new(most: usize) -> Self {
        Self { asked: Mutex::new(Vec::new()), most, full: AtomicBool::new(false) }
    }

    fn take(&self, object: &Arc<dyn Object>, within: Duration, progress: &Progress) -> Turn {
        let mut asked = self.asked.lock();
        if progress.abandoned.load(Ordering::Relaxed) {
            return Turn::Stop;
        }
        if let Some(question) = asked.iter().find(|question| object.same(&*question.object)) {
            let until = question.since + within;
            return if until > Instant::now() { Turn::Wait(Arc::clone(&question.answer), until) } else { Turn::Skip };
        }
        if asked.len() >= self.most {
            return Turn::Full;
        }
        let answer = Arc::new(Slot::default());
        let since = Instant::now();
        asked.push(Question { object: Arc::clone(object), since, answer: Arc::clone(&answer) });
        Turn::Ask(answer)
    }

    fn answered(&self, answer: &Arc<Slot>, path: Option<PathBuf>, progress: &Progress) -> bool {
        let mut asked = self.asked.lock();
        asked.retain(|question| !Arc::ptr_eq(&question.answer, answer));
        answer.give(path);
        progress.pass()
    }

    fn passed(&self, progress: &Progress) -> bool {
        let _asked = self.asked.lock();
        progress.pass()
    }

    fn give_up(&self, answered: usize, progress: &Progress) -> bool {
        let _asked = self.asked.lock();
        if progress.done.load(Ordering::Relaxed) != answered {
            return false;
        }
        progress.abandoned.store(true, Ordering::Relaxed);
        true
    }

    fn filled(&self) -> bool {
        !self.full.swap(true, Ordering::Relaxed)
    }
}

fn word(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from_ne_bytes(bytes.get(at..at.checked_add(WORD)?)?.try_into().ok()?))
}

fn number(entry: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(entry.get(at..at + 4)?.try_into().ok()?))
}

fn listed(snapshot: &[u8]) -> impl Iterator<Item = &[u8; ENTRY]> {
    let count = word(snapshot, 0).unwrap_or(0);
    snapshot.get(HEADER..).unwrap_or_default().as_chunks::<ENTRY>().0.iter().take(count)
}

fn entries(snapshot: &[u8]) -> impl Iterator<Item = (usize, u32)> {
    listed(snapshot).filter_map(|entry| Some((word(entry, 0)?, number(entry, TYPE_INDEX)?)))
}

fn handed_down(snapshot: &[u8]) -> Vec<usize> {
    let inherits =
        |entry: &&[u8; ENTRY]| number(*entry, ATTRIBUTES).is_some_and(|attributes| attributes & INHERIT != 0);
    listed(snapshot).filter(inherits).filter_map(|entry| word(entry, 0)).collect()
}

fn of_type(snapshot: &[u8], wanted: u32) -> Vec<usize> {
    entries(snapshot).filter(|(_, index)| *index == wanted).map(|(handle, _)| handle).collect()
}

fn type_of(snapshot: &[u8], handle: usize) -> Option<u32> {
    entries(snapshot).find(|(value, _)| *value == handle).map(|(_, index)| index)
}

fn names(handles: Vec<usize>, asking: &Asking) -> Vec<PathBuf> {
    let mut left = handles;
    let mut found = Vec::new();
    while !left.is_empty() {
        let Some(answered) = batch(&left, asking, &mut found) else { break };
        left = left.split_off(answered.saturating_add(1).min(left.len()));
    }
    found
}

fn batch(handles: &[usize], asking: &Asking, found: &mut Vec<PathBuf>) -> Option<usize> {
    let progress = Arc::new(Progress::default());
    let answers = start(handles.to_vec(), asking.clone(), Arc::clone(&progress))?;
    let mut answered = 0;
    loop {
        match answers.recv_timeout(asking.within) {
            Ok(Answer::Named(path)) => {
                found.extend(path);
                answered += 1;
            }
            Ok(Answer::Full) => return full(asking.waiting),
            Err(RecvTimeoutError::Timeout) if !asking.waiting.give_up(answered, &progress) => {}
            Err(_) => return Some(answered),
        }
    }
}

fn full(waiting: &Waiting) -> Option<usize> {
    if waiting.filled() {
        log::warning!("process", "open files are not named, too many names wait", most = waiting.most);
    }
    None
}

fn start(handles: Vec<usize>, asking: Asking, progress: Arc<Progress>) -> Option<Receiver<Answer>> {
    let (answers, answered) = mpsc::channel();
    let work = move || {
        for handle in handles {
            let Some(answer) = answer(handle, &asking, &progress) else { return };
            let full = matches!(answer, Answer::Full);
            if answers.send(answer).is_err() || full {
                return;
            }
        }
    };
    thread::Builder::new().name("open files".to_string()).spawn(work).ok()?;
    Some(answered)
}

fn answer(handle: usize, asking: &Asking, progress: &Progress) -> Option<Answer> {
    let Some(object) = (asking.open)(handle) else {
        return asking.waiting.passed(progress).then_some(Answer::Named(None));
    };
    let path = match asking.waiting.take(&object, asking.within, progress) {
        Turn::Stop => return None,
        Turn::Full => return Some(Answer::Full),
        Turn::Ask(answer) => return ask(&object, &answer, asking.waiting, progress),
        Turn::Wait(answer, until) => answer.wait(until),
        Turn::Skip => None,
    };
    asking.waiting.passed(progress).then_some(Answer::Named(path))
}

fn ask(object: &Arc<dyn Object>, answer: &Arc<Slot>, waiting: &Waiting, progress: &Progress) -> Option<Answer> {
    let path = panics::contain(|| object.name()).flatten();
    waiting.answered(answer, path.clone(), progress).then_some(Answer::Named(path))
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::fs::File;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::PathBuf;
    use std::ptr;
    use std::sync::{Arc, OnceLock};

    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessHandleInformation};
    use windows_sys::Win32::Foundation::{
        CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, HANDLE_FLAG_INHERIT,
        STATUS_INFO_LENGTH_MISMATCH, SetHandleInformation,
    };
    use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_DOS};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION};

    use super::{Asking, NAME_WITHIN, Object, WAITING, handed_down, names, of_type, type_of};
    use crate::process::imp::{Owned, as_bytes};

    const SNAPSHOT_FIRST_WORDS: usize = 1 << 11;
    const SNAPSHOT_TRIES: usize = 4;
    const PATH_FIRST_UNITS: usize = 512;
    const PATH_TRIES: usize = 3;

    struct Process(Owned);

    unsafe impl Send for Process {}
    unsafe impl Sync for Process {}

    impl Process {
        fn object(&self, handle: usize) -> Option<Arc<dyn Object>> {
            Some(Arc::new(duplicate(self.0.0, handle)?))
        }
    }

    pub(super) struct Duplicate(Owned);

    unsafe impl Send for Duplicate {}
    unsafe impl Sync for Duplicate {}

    impl Object for Duplicate {
        fn name(&self) -> Option<PathBuf> {
            final_path(self.0.0)
        }

        fn same(&self, other: &dyn Object) -> bool {
            unsafe { CompareObjectHandles(self.0.0, ptr::without_provenance_mut(other.raw())) != 0 }
        }

        fn raw(&self) -> usize {
            self.0.0.addr()
        }
    }

    pub(crate) fn open_files(pid: i32) -> Vec<PathBuf> {
        let Some(file) = file_type() else { return Vec::new() };
        let Some(process) = Owned::process(pid, PROCESS_QUERY_INFORMATION | PROCESS_DUP_HANDLE) else {
            return Vec::new();
        };
        let Some(snapshot) = snapshot(process.0) else { return Vec::new() };
        let process = Process(process);
        let asking =
            Asking { open: Arc::new(move |handle| process.object(handle)), within: NAME_WITHIN, waiting: &WAITING };
        names(of_type(as_bytes(&snapshot), file), &asking)
    }

    pub(crate) fn hand_down_nothing() {
        for handle in own_handed_down() {
            unsafe { SetHandleInformation(ptr::without_provenance_mut(handle), HANDLE_FLAG_INHERIT, 0) };
        }
    }

    pub(super) fn own_handed_down() -> Vec<usize> {
        snapshot(unsafe { GetCurrentProcess() }).map(|snapshot| handed_down(as_bytes(&snapshot))).unwrap_or_default()
    }

    fn file_type() -> Option<u32> {
        static FILE: OnceLock<u32> = OnceLock::new();
        if let Some(index) = FILE.get() {
            return Some(*index);
        }
        let probe = File::open("NUL").ok()?;
        let snapshot = snapshot(unsafe { GetCurrentProcess() })?;
        let index = type_of(as_bytes(&snapshot), probe.as_raw_handle().addr())?;
        Some(*FILE.get_or_init(|| index))
    }

    fn snapshot(process: HANDLE) -> Option<Vec<u64>> {
        let mut words = vec![0_u64; SNAPSHOT_FIRST_WORDS];
        for _ in 0..SNAPSHOT_TRIES {
            let (size, mut needed) = (u32::try_from(size_of_val(words.as_slice())).ok()?, 0_u32);
            let into = words.as_mut_ptr().cast();
            let status =
                unsafe { NtQueryInformationProcess(process, ProcessHandleInformation, into, size, &raw mut needed) };
            if status >= 0 {
                return Some(words);
            }
            if status != STATUS_INFO_LENGTH_MISMATCH {
                return None;
            }
            let grown = usize::try_from(needed).ok()?.div_ceil(size_of::<u64>()).max(words.len());
            words = vec![0; grown.saturating_mul(2)];
        }
        None
    }

    pub(super) fn duplicate(from: HANDLE, handle: usize) -> Option<Duplicate> {
        let mut copy = ptr::null_mut();
        let (source, here) = (ptr::without_provenance_mut(handle), unsafe { GetCurrentProcess() });
        let copied = unsafe { DuplicateHandle(from, source, here, &raw mut copy, 0, 0, DUPLICATE_SAME_ACCESS) } != 0;
        if !copied {
            return None;
        }
        Owned::new(copy).map(Duplicate)
    }

    fn final_path(handle: HANDLE) -> Option<PathBuf> {
        let mut units = vec![0_u16; PATH_FIRST_UNITS];
        for _ in 0..PATH_TRIES {
            let size = u32::try_from(units.len()).ok()?;
            let length = unsafe { GetFinalPathNameByHandleW(handle, units.as_mut_ptr(), size, VOLUME_NAME_DOS) };
            let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
            if length < units.len() {
                return Some(PathBuf::from(OsString::from_wide(&units[..length])));
            }
            units = vec![0; length];
        }
        None
    }
}

#[cfg(test)]
mod tests;
