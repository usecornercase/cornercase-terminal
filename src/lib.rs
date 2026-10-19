pub mod error;
pub(crate) mod gh;
pub(crate) mod http;
pub(crate) mod list;
pub(crate) mod log;
pub(crate) mod panics;
pub mod paths;
pub(crate) mod persist;
pub(crate) mod process;
pub(crate) mod text;
pub(crate) mod time;
pub mod version;

pub(crate) mod accounts;
pub mod agents;
pub(crate) mod changes;
pub mod config;
pub(crate) mod discord;
pub(crate) mod files;
pub(crate) mod flows;
pub(crate) mod git;
pub(crate) mod graphics;
pub(crate) mod highlight;
pub mod host;
pub(crate) mod issues;
pub(crate) mod messenger;
pub(crate) mod plugins;
pub mod project;
pub(crate) mod pull_request;
pub(crate) mod queue;
pub(crate) mod settings;
pub(crate) mod shortcuts;
pub(crate) mod star;
pub mod state;
pub(crate) mod telegram;
pub(crate) mod term;
pub(crate) mod todo;
pub(crate) mod tour;
pub mod update;

pub mod ui;

pub(crate) mod app;

pub mod cli;
pub(crate) mod client;
pub mod control;
pub mod protocol;
pub(crate) mod remote;
pub(crate) mod server;
pub(crate) mod web;

#[cfg(test)]
mod test_util;
