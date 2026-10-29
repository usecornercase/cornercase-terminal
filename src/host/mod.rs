pub mod clipboard;
#[cfg_attr(not(test), expect(dead_code, reason = "the machines' links watch the route in the next commits"))]
pub(crate) mod network;
pub(crate) mod notify;
pub(crate) mod open;
pub(crate) mod pointer;
pub(crate) mod sound;
pub(crate) mod theme;
