use std::io::{self, BufRead as _, BufReader, Read};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const QUEUED: usize = 1024;

pub(super) type Lines = Receiver<io::Result<Vec<u8>>>;

pub(super) fn read(source: impl Read + Send + 'static) -> Lines {
    let (tx, lines) = mpsc::sync_channel(QUEUED);
    thread::spawn(move || {
        for line in BufReader::new(source).split(b'\n') {
            let failed = line.is_err();
            if tx.send(line).is_err() || failed {
                return;
            }
        }
    });
    lines
}

#[cfg(test)]
mod tests;
