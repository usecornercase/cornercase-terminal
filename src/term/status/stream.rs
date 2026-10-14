const MAX_SEQUENCE: usize = 4096;

#[derive(Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    String,
    StringEscape,
}

#[derive(Default)]
pub(super) struct Stream {
    state: State,
    bytes: Vec<u8>,
    overflow: bool,
}

pub(super) enum Command<'a> {
    Reset,
    Osc(&'a [u8]),
}

impl Stream {
    pub(super) fn advance(&mut self, byte: u8) -> Option<Command<'_>> {
        if matches!(byte, 0x18 | 0x1a) {
            self.state = State::Ground;
            return None;
        }
        match self.state {
            State::Ground if byte == 0x1b => self.state = State::Escape,
            State::Escape => match byte {
                b']' => {
                    self.bytes.clear();
                    self.overflow = false;
                    self.state = State::Osc;
                }
                b'[' | 0x20..=0x2f => self.state = State::Csi,
                b'P' | b'_' | b'^' | b'X' => self.state = State::String,
                b'c' => {
                    self.state = State::Ground;
                    return Some(Command::Reset);
                }
                0x30..=0x7e => self.state = State::Ground,
                _ => {}
            },
            State::Csi => match byte {
                0x1b => self.state = State::Escape,
                0x40..=0x7e => self.state = State::Ground,
                _ => {}
            },
            State::Osc => match byte {
                0x07 => return self.finish(3),
                0x1b => self.state = State::OscEscape,
                _ if self.bytes.len() < MAX_SEQUENCE => self.bytes.push(byte),
                _ => self.overflow = true,
            },
            State::OscEscape if byte == b'\\' => return self.finish(4),
            State::OscEscape => {
                self.state = State::Escape;
                return self.advance(byte);
            }
            State::String if byte == 0x1b => self.state = State::StringEscape,
            State::StringEscape => {
                self.state = if byte == b'\\' { State::Ground } else { State::String };
            }
            _ => {}
        }
        None
    }

    fn finish(&mut self, framing: usize) -> Option<Command<'_>> {
        self.state = State::Ground;
        (!self.overflow && self.bytes.len() + framing <= MAX_SEQUENCE).then_some(Command::Osc(&self.bytes))
    }
}

#[cfg(test)]
mod tests;
