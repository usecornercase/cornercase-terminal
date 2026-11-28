use std::cell::Cell;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::{ServerConfig, ServerConnection};

use crate::log;
use crate::web::http::Timed;

const HANDSHAKE: u8 = 0x16;

pub(super) enum Link {
    Plain(TcpStream),
    Tls(Box<Encrypted>),
}

pub(super) struct Encrypted {
    connection: ServerConnection,
    socket: TcpStream,
    wait: Cell<Option<Duration>>,
}

impl Link {
    pub(super) fn accept(
        stream: TcpStream,
        config: impl FnOnce() -> Option<Arc<ServerConfig>>,
        deadline: Instant,
    ) -> Option<Self> {
        if first_byte(&stream, deadline)? != HANDSHAKE {
            return Some(Self::Plain(stream));
        }
        let connection = ServerConnection::new(config()?).ok()?;
        let mut encrypted = Encrypted { connection, socket: stream, wait: Cell::new(None) };
        match encrypted.handshake(deadline) {
            Ok(()) => Some(Self::Tls(Box::new(encrypted))),
            Err(e) => {
                log::debug!("web", "phone app TLS handshake failed", error = e);
                None
            }
        }
    }

    pub(super) fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(tls) => &tls.socket,
        }
    }

    pub(super) fn encrypted(&self) -> bool {
        matches!(self, Self::Tls(_))
    }

    pub(super) fn end(&mut self) {
        if let Self::Tls(tls) = self {
            tls.connection.send_close_notify();
            let _ = tls.send();
        }
        let _ = self.tcp().shutdown(Shutdown::Write);
    }
}

impl Encrypted {
    fn handshake(&mut self, deadline: Instant) -> io::Result<()> {
        while self.connection.is_handshaking() {
            self.send()?;
            if !self.connection.is_handshaking() {
                break;
            }
            self.receive(Some(deadline))?;
        }
        self.send()
    }

    fn send(&mut self) -> io::Result<()> {
        while self.connection.wants_write() {
            self.connection.write_tls(&mut self.socket)?;
        }
        Ok(())
    }

    fn receive(&mut self, deadline: Option<Instant>) -> io::Result<()> {
        if let Some(deadline) = deadline {
            let time = left(deadline).ok_or_else(|| io::Error::from(ErrorKind::WouldBlock))?;
            self.socket.set_read_timeout(Some(time))?;
        }
        if self.connection.read_tls(&mut self.socket)? == 0 {
            return Err(ErrorKind::UnexpectedEof.into());
        }
        self.connection.process_new_packets().map_err(io::Error::other)?;
        Ok(())
    }
}

fn left(deadline: Instant) -> Option<Duration> {
    Some(deadline.saturating_duration_since(Instant::now())).filter(|left| !left.is_zero())
}

fn first_byte(stream: &TcpStream, deadline: Instant) -> Option<u8> {
    stream.set_read_timeout(Some(left(deadline)?)).ok()?;
    let mut byte = [0_u8; 1];
    match stream.peek(&mut byte) {
        Ok(1) => Some(byte[0]),
        _ => None,
    }
}

impl Read for Encrypted {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let deadline = self.wait.get().map(|wait| Instant::now() + wait);
        loop {
            match self.connection.reader().read(buffer) {
                Err(e) if e.kind() == ErrorKind::WouldBlock => self.receive(deadline)?,
                done => return done,
            }
        }
    }
}

impl Write for Encrypted {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.send()?;
        self.connection.writer().write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send()
    }
}

impl Read for Link {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            Self::Tls(tls) => tls.read(buffer),
        }
    }
}

impl Write for Link {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            Self::Tls(tls) => tls.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(tls) => tls.flush(),
        }
    }
}

impl Timed for Link {
    fn wait_at_most(&self, time: Duration) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.set_read_timeout(Some(time)),
            Self::Tls(tls) => {
                tls.wait.set(Some(time));
                Ok(())
            }
        }
    }
}
