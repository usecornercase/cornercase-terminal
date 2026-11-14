use std::io::{self, Read, Write};
use std::process::{Child, ExitStatus};

use super::{MACHINE, Remote};

const UPDATE_LINE: &str = "cornercase update";
const READ: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::remote) enum Printed {
    Data,
    Json,
    Messages,
}

pub(in crate::remote) fn printed(words: &[&str]) -> Printed {
    if words.first() == Some(&"logs") || words.starts_with(&["plugin", "logs"]) {
        return Printed::Data;
    }
    let options = words.iter().take_while(|word| **word != "--");
    if options.into_iter().any(|word| *word == "--json") { Printed::Json } else { Printed::Messages }
}

struct Hinting<'a, W: Write> {
    remote: &'a Remote,
    out: W,
    held: Vec<u8>,
    mid_line: bool,
}

impl<'a, W: Write> Hinting<'a, W> {
    fn new(remote: &'a Remote, out: W) -> Self {
        Self { remote, out, held: Vec::new(), mid_line: false }
    }

    fn take(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.held.extend_from_slice(bytes);
        if let Some(end) = self.held.iter().rposition(|byte| *byte == b'\n') {
            let lines: Vec<u8> = self.held.drain(..=end).collect();
            for line in lines.split_inclusive(|byte| *byte == b'\n') {
                self.line(line)?;
                self.mid_line = false;
            }
        }
        let ready = ready(&self.held);
        let partial: Vec<u8> = self.held.drain(..ready).collect();
        self.mid_line |= !partial.is_empty();
        self.write(&self.remote.hinted(&String::from_utf8_lossy(&partial)))
    }

    fn line(&mut self, line: &[u8]) -> io::Result<()> {
        let text = String::from_utf8_lossy(line);
        let bare = text.trim_end_matches('\n').trim_end_matches('\r');
        if bare == UPDATE_LINE && !self.mid_line {
            let ending = &text[bare.len()..];
            return self.write(&format!("{}{ending}", self.remote.update_line()));
        }
        self.write(&self.remote.hinted(&text))
    }

    fn finish(mut self) -> io::Result<W> {
        let held = std::mem::take(&mut self.held);
        if !held.is_empty() {
            self.line(&held)?;
        }
        self.out.flush()?;
        Ok(self.out)
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.out.write_all(text.as_bytes())?;
        self.out.flush()
    }
}

fn ready(partial: &[u8]) -> usize {
    let line = partial.strip_suffix(b"\r").unwrap_or(partial);
    if !line.is_empty() && UPDATE_LINE.as_bytes().starts_with(line) {
        return 0;
    }
    let ticks: Vec<usize> = partial.iter().enumerate().filter(|(_, byte)| **byte == b'`').map(|(at, _)| at).collect();
    if ticks.len() % 2 == 1 {
        return ticks[ticks.len() - 1];
    }
    if let [.., open, close] = ticks[..]
        && partial[open + 1..].starts_with(b"cornercase ")
        && undecided(&partial[close + 1..])
    {
        return open;
    }
    match std::str::from_utf8(partial) {
        Err(why) if why.error_len().is_none() => why.valid_up_to(),
        _ => partial.len(),
    }
}

fn undecided(after: &[u8]) -> bool {
    MACHINE.iter().any(|word| {
        let word = format!(" {word}");
        let word = word.as_bytes();
        after.len() <= word.len() && (word.starts_with(after) || after.starts_with(word))
    })
}

fn copy(remote: &Remote, mut from: impl Read, to: impl Write) -> io::Result<()> {
    let mut hinting = Hinting::new(remote, to);
    let mut buffer = [0; READ];
    loop {
        match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hinting.take(&buffer[..read])?,
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(why) => return Err(why),
        }
    }
    hinting.finish().map(drop)
}

pub(in crate::remote) fn relayed(
    remote: &Remote,
    child: &mut Child,
    out: impl Write + Send,
    err: impl Write + Send,
) -> io::Result<ExitStatus> {
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    std::thread::scope(|scope| {
        let outs = stdout.map(|from| scope.spawn(|| copy(remote, from, out)));
        let errs = stderr.map(|from| scope.spawn(|| copy(remote, from, err)));
        let status = child.wait();
        let mut failed = None;
        for copying in [outs, errs].into_iter().flatten() {
            if let Ok(Err(why)) = copying.join() {
                failed.get_or_insert(why);
            }
        }
        let status = status?;
        failed.map_or(Ok(status), Err)
    })
}

#[cfg(test)]
mod tests;
