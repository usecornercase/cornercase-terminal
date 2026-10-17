mod archive;
pub(crate) mod install;
pub(crate) mod manifest;
pub(crate) mod registry;
pub(crate) mod run;
pub(crate) mod source;
pub(crate) mod supervise;

pub(crate) use manifest::Context;
pub(crate) use registry::{Plugin, Registry, Watch};
pub(crate) use run::Places;
pub(crate) use supervise::{Kind, Supervisor};
