use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use crate::{log, panics};

#[cfg(windows)]
pub(crate) use imp::open_files;

#[cfg(windows)]
const NAME_WITHIN: Duration = Duration::from_secs(1);
#[cfg(windows)]
const MOST_ASKED: usize = 16;
const WORD: usize = size_of::<usize>();
const HEADER: usize = 2 * WORD;
const TYPE_INDEX: usize = 3 * WORD + 4;
const ENTRY: usize = 3 * WORD + 16;

#[cfg(windows)]
static WAITING: Waiting = Waiting::new(MOST_ASKED);

type Ask = Arc<dyn Fn(usize) -> Option<PathBuf> + Send + Sync>;

#[derive(Clone)]
struct Asking {
    ask: Ask,
    within: Duration,
    waiting: &'static Waiting,
}

struct Waiting {
    asked: Mutex<Vec<(i32, usize)>>,
    most: usize,
    full: AtomicBool,
}

#[derive(Default)]
struct Progress {
    done: AtomicUsize,
    abandoned: AtomicBool,
}

enum Turn {
    Ask,
    Skip,
    Full,
    Stop,
}

enum Answer {
    Named(Option<PathBuf>),
    Full,
}

impl Waiting {
    const fn new(most: usize) -> Self {
        Self { asked: Mutex::new(Vec::new()), most, full: AtomicBool::new(false) }
    }

    fn take(&self, pid: i32, handle: usize, progress: &Progress) -> Turn {
        let mut asked = self.asked.lock();
        if progress.abandoned.load(Ordering::Relaxed) {
            return Turn::Stop;
        }
        if asked.contains(&(pid, handle)) {
            progress.done.fetch_add(1, Ordering::Relaxed);
            return Turn::Skip;
        }
        if asked.len() >= self.most {
            return Turn::Full;
        }
        asked.push((pid, handle));
        Turn::Ask
    }

    fn answered(&self, pid: i32, handle: usize, progress: &Progress) -> bool {
        let mut asked = self.asked.lock();
        if let Some(at) = asked.iter().position(|entry| *entry == (pid, handle)) {
            asked.swap_remove(at);
        }
        progress.done.fetch_add(1, Ordering::Relaxed);
        !progress.abandoned.load(Ordering::Relaxed)
    }

    fn give_up(&self, answered: usize, progress: &Progress) -> bool {
        let _asked = self.asked.lock();
        if progress.done.load(Ordering::Relaxed) != answered {
            return false;
        }
        progress.abandoned.store(true, Ordering::Relaxed);
        true
    }

    fn filled(&self) {
        if !self.full.swap(true, Ordering::Relaxed) {
            log::warning!("process", "open files are not named, too many names wait", most = self.most);
        }
    }
}

fn word(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from_ne_bytes(bytes.get(at..at.checked_add(WORD)?)?.try_into().ok()?))
}

fn type_index(entry: &[u8]) -> Option<u32> {
    Some(u32::from_ne_bytes(entry.get(TYPE_INDEX..TYPE_INDEX + 4)?.try_into().ok()?))
}

fn entries(snapshot: &[u8]) -> impl Iterator<Item = (usize, u32)> {
    let count = word(snapshot, 0).unwrap_or(0);
    let listed = snapshot.get(HEADER..).unwrap_or_default().as_chunks::<ENTRY>().0.iter().take(count);
    listed.filter_map(|entry| Some((word(entry, 0)?, type_index(entry)?)))
}

fn of_type(snapshot: &[u8], wanted: u32) -> Vec<usize> {
    entries(snapshot).filter(|(_, index)| *index == wanted).map(|(handle, _)| handle).collect()
}

fn type_of(snapshot: &[u8], handle: usize) -> Option<u32> {
    entries(snapshot).find(|(value, _)| *value == handle).map(|(_, index)| index)
}

fn names(pid: i32, handles: Vec<usize>, asking: &Asking) -> Vec<PathBuf> {
    let mut left = handles;
    let mut found = Vec::new();
    while !left.is_empty() {
        let Some(answered) = batch(pid, &left, asking, &mut found) else { break };
        left = left.split_off(answered.saturating_add(1).min(left.len()));
    }
    found
}

fn batch(pid: i32, handles: &[usize], asking: &Asking, found: &mut Vec<PathBuf>) -> Option<usize> {
    let progress = Arc::new(Progress::default());
    let answers = start(pid, handles.to_vec(), asking.clone(), Arc::clone(&progress))?;
    let mut answered = 0;
    loop {
        match answers.recv_timeout(asking.within) {
            Ok(Answer::Named(path)) => {
                found.extend(path);
                answered += 1;
            }
            Ok(Answer::Full) => {
                asking.waiting.filled();
                return None;
            }
            Err(RecvTimeoutError::Timeout) if !asking.waiting.give_up(answered, &progress) => {}
            Err(_) => return Some(answered),
        }
    }
}

fn start(pid: i32, handles: Vec<usize>, asking: Asking, progress: Arc<Progress>) -> Option<Receiver<Answer>> {
    let (answers, answered) = mpsc::channel();
    let work = move || {
        for handle in handles {
            let Some(answer) = answer(pid, handle, &asking, &progress) else { return };
            let full = matches!(answer, Answer::Full);
            if answers.send(answer).is_err() || full {
                return;
            }
        }
    };
    thread::Builder::new().name("open files".to_string()).spawn(work).ok()?;
    Some(answered)
}

fn answer(pid: i32, handle: usize, asking: &Asking, progress: &Progress) -> Option<Answer> {
    match asking.waiting.take(pid, handle, progress) {
        Turn::Stop => None,
        Turn::Full => Some(Answer::Full),
        Turn::Skip => Some(Answer::Named(None)),
        Turn::Ask => {
            let path = panics::contain(|| (asking.ask)(handle)).flatten();
            asking.waiting.answered(pid, handle, progress).then_some(Answer::Named(path))
        }
    }
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
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, STATUS_INFO_LENGTH_MISMATCH};
    use windows_sys::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_DOS};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_DUP_HANDLE, PROCESS_QUERY_INFORMATION};

    use super::{Asking, NAME_WITHIN, WAITING, names, of_type, type_of};
    use crate::process::imp::{Owned, as_bytes};

    const SNAPSHOT_FIRST_WORDS: usize = 1 << 11;
    const SNAPSHOT_TRIES: usize = 4;
    const PATH_FIRST_UNITS: usize = 512;
    const PATH_TRIES: usize = 3;

    struct Process(Owned);

    unsafe impl Send for Process {}
    unsafe impl Sync for Process {}

    pub(crate) fn open_files(pid: i32) -> Vec<PathBuf> {
        let Some(file) = file_type() else { return Vec::new() };
        let Some(process) = Owned::process(pid, PROCESS_QUERY_INFORMATION | PROCESS_DUP_HANDLE) else {
            return Vec::new();
        };
        let Some(snapshot) = snapshot(process.0) else { return Vec::new() };
        let process = Process(process);
        let asking =
            Asking { ask: Arc::new(move |handle| name(&process, handle)), within: NAME_WITHIN, waiting: &WAITING };
        names(pid, of_type(as_bytes(&snapshot), file), &asking)
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

    fn name(process: &Process, handle: usize) -> Option<PathBuf> {
        let mut copy = ptr::null_mut();
        let (source, here) = (ptr::without_provenance_mut(handle), unsafe { GetCurrentProcess() });
        let copied =
            unsafe { DuplicateHandle(process.0.0, source, here, &raw mut copy, 0, 0, DUPLICATE_SAME_ACCESS) } != 0;
        if !copied {
            return None;
        }
        final_path(Owned::new(copy)?.0)
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
