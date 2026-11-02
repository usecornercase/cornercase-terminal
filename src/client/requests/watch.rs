pub(crate) use imp::{input_ended, reader_gone_within, reader_left, watched};

#[cfg(unix)]
mod imp {
    use std::time::Duration;

    use rustix::event::{PollFd, PollFlags, Timespec};
    use rustix::fs::FileType;
    use rustix::io::Errno;

    use crate::os::console::BorrowedFd;

    pub(crate) fn watched(out: BorrowedFd<'_>) -> Option<BorrowedFd<'_>> {
        let kind = rustix::fs::fstat(out).map(|stat| FileType::from_raw_mode(stat.st_mode));
        matches!(kind, Ok(FileType::Fifo | FileType::Socket)).then_some(out)
    }

    pub(crate) fn reader_left(server: BorrowedFd<'_>, out: &mut Option<BorrowedFd<'_>>) -> bool {
        while let Some(reader) = *out {
            let Some((heard, left)) = polled(Some(server), reader, None) else { return false };
            if left.intersects(PollFlags::HUP | PollFlags::ERR) {
                return true;
            }
            if !left.is_empty() {
                *out = None;
            }
            if heard {
                return false;
            }
        }
        false
    }

    pub(crate) fn reader_gone_within(out: &mut Option<BorrowedFd<'_>>, wait: Duration) -> bool {
        let Some(reader) = *out else {
            std::thread::sleep(wait);
            return false;
        };
        let Some((_, left)) = polled(None, reader, Some(wait)) else {
            *out = None;
            return false;
        };
        if !left.is_empty() && !left.intersects(PollFlags::HUP | PollFlags::ERR) {
            *out = None;
        }
        left.intersects(PollFlags::HUP | PollFlags::ERR)
    }

    pub(crate) fn input_ended(
        server: Option<BorrowedFd<'_>>,
        input: &mut Option<BorrowedFd<'_>>,
        wait: Option<Duration>,
    ) -> bool {
        while let Some(fd) = *input {
            let Some((heard, events)) = polled(server, fd, wait) else {
                *input = None;
                return false;
            };
            match input_state(fd, events) {
                Some(true) => return true,
                Some(false) => {}
                None => *input = None,
            }
            if heard || events.is_empty() {
                return false;
            }
        }
        false
    }

    fn input_state(fd: BorrowedFd<'_>, events: PollFlags) -> Option<bool> {
        if events.intersects(PollFlags::HUP | PollFlags::ERR) {
            return Some(true);
        }
        if events.is_empty() {
            return Some(false);
        }
        if events != PollFlags::IN {
            return None;
        }
        let mut buffer = [0u8; 64];
        match rustix::io::read(fd, &mut buffer) {
            Ok(0) => Some(true),
            Ok(_) | Err(Errno::INTR | Errno::AGAIN) => Some(false),
            Err(_) => None,
        }
    }

    fn polled(
        server: Option<BorrowedFd<'_>>,
        reader: BorrowedFd<'_>,
        wait: Option<Duration>,
    ) -> Option<(bool, PollFlags)> {
        let wait = match wait {
            Some(wait) => Some(Timespec::try_from(wait).ok()?),
            None => None,
        };
        let mut fds = vec![PollFd::new(&reader, PollFlags::IN | PollFlags::HUP)];
        fds.extend(server.as_ref().map(|server| PollFd::new(server, PollFlags::IN)));
        loop {
            match rustix::event::poll(&mut fds, wait.as_ref()) {
                Ok(_) => break,
                Err(Errno::INTR) => {}
                Err(_) => return None,
            }
        }
        Some((fds.get(1).is_some_and(|fd| !fd.revents().is_empty()), fds[0].revents()))
    }
}

#[cfg(not(unix))]
mod imp {
    use std::time::Duration;

    use crate::os::console::BorrowedFd;

    pub(crate) fn watched(_out: BorrowedFd<'_>) -> Option<BorrowedFd<'_>> {
        None
    }

    pub(crate) fn reader_left(_server: BorrowedFd<'_>, _out: &mut Option<BorrowedFd<'_>>) -> bool {
        false
    }

    pub(crate) fn reader_gone_within(_out: &mut Option<BorrowedFd<'_>>, wait: Duration) -> bool {
        std::thread::sleep(wait);
        false
    }

    pub(crate) fn input_ended(
        _server: Option<BorrowedFd<'_>>,
        _input: &mut Option<BorrowedFd<'_>>,
        _wait: Option<Duration>,
    ) -> bool {
        false
    }
}
