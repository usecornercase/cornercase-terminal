use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};

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
}

impl<'a, W: Write> Hinting<'a, W> {
    fn new(remote: &'a Remote, out: W) -> Self {
        Self { remote, out, held: Vec::new() }
    }

    fn take(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.held.extend_from_slice(bytes);
        if let Some(end) = self.held.iter().rposition(|byte| *byte == b'\n') {
            let lines: Vec<u8> = self.held.drain(..=end).collect();
            for line in lines.split_inclusive(|byte| *byte == b'\n') {
                self.line(line)?;
            }
        }
        let ready = ready(&self.held);
        let partial: Vec<u8> = self.held.drain(..ready).collect();
        self.write(&self.remote.hinted(&String::from_utf8_lossy(&partial)))
    }

    fn line(&mut self, line: &[u8]) -> io::Result<()> {
        let text = String::from_utf8_lossy(line);
        let bare = text.trim_end_matches('\n').trim_end_matches('\r');
        if bare == UPDATE_LINE {
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
    if !partial.is_empty() && UPDATE_LINE.as_bytes().starts_with(partial) {
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

fn copy(remote: &Remote, mut from: impl Read, to: impl Write) {
    let mut hinting = Hinting::new(remote, to);
    let mut buffer = [0; READ];
    loop {
        match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if hinting.take(&buffer[..read]).is_err() {
                    return;
                }
            }
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let _ = hinting.finish();
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
        for copying in [outs, errs].into_iter().flatten() {
            let _ = copying.join();
        }
        status
    })
}

pub(in crate::remote) fn status(
    remote: &Remote,
    mut command: Command,
    printed: Printed,
    tied: bool,
) -> io::Result<ExitStatus> {
    if tied {
        command.stdin(Stdio::piped());
    }
    if printed != Printed::Data {
        command.stderr(Stdio::piped());
    }
    if printed == Printed::Messages {
        command.stdout(Stdio::piped());
    }
    let mut child = command.spawn()?;
    let held = child.stdin.take();
    let status = relayed(remote, &mut child, io::stdout(), io::stderr());
    drop(held);
    status
}

#[cfg(test)]
mod tests;
