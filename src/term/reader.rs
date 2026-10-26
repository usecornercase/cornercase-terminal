use std::io::Read;
use std::ops::Deref;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::thread;

use super::Event;

const READ_BUFFER: usize = 16 * 1024;
const OUTPUT_CHUNKS: usize = 4;

#[derive(Debug)]
pub(crate) struct Output {
    bytes: Vec<u8>,
    ready: Option<SyncSender<()>>,
}

impl Deref for Output {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        if let Some(ready) = &self.ready {
            let _ = ready.try_send(());
        }
    }
}

#[cfg(test)]
impl From<Vec<u8>> for Output {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes, ready: None }
    }
}

pub(super) fn spawn<E: From<Event> + Send + 'static>(
    id: u64,
    generation: u32,
    mut reader: Box<dyn Read + Send>,
    tx: Sender<E>,
) {
    let (ready, available) = mpsc::sync_channel(OUTPUT_CHUNKS);
    for _ in 0..OUTPUT_CHUNKS {
        let _ = ready.send(());
    }
    thread::spawn(move || {
        let mut buf = vec![0u8; READ_BUFFER];
        while available.recv().is_ok() {
            let Ok(n @ 1..) = reader.read(&mut buf) else { break };
            let output = Output { bytes: buf[..n].to_vec(), ready: Some(ready.clone()) };
            if tx.send(E::from(Event::Output(id, generation, output))).is_err() {
                return;
            }
        }
        let _ = tx.send(E::from(Event::Exited(id, generation)));
    });
}

#[cfg(test)]
mod tests;
