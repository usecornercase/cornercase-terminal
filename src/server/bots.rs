use super::Server;
use crate::protocol::{ClientMessage, ServerMessage};

impl Server {
    pub(super) fn refused(&mut self, id: u64, message: &ClientMessage) -> bool {
        if let Some(why) = self.app.refused_stop(id, message) {
            self.send_to(id, ServerMessage::Rejected(why.into()));
        } else if let Some(why) = self.app.refused_window(id, message) {
            self.reject(id, why);
        } else {
            return false;
        }
        true
    }
}
