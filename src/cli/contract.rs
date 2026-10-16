use std::fmt::Write as _;

use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{self, contract::Descriptor};
use crate::error::Result;

pub(super) fn run(json: bool) -> Result<()> {
    let value = ask("contract", control::Command::Contract(control::Contract {}))?;
    if json {
        return print_json(&value);
    }
    print_out(&summary(&answer::<Descriptor>(value)?))
}

pub(crate) fn summary(descriptor: &Descriptor) -> String {
    let mut text =
        format!("cornercase {}\ncontract {}\nboot {}\n", descriptor.version, descriptor.contract, descriptor.boot);
    let off: Vec<_> = descriptor.operations.iter().filter(|operation| !operation.available).collect();
    let _ = writeln!(
        text,
        "commands {} ({} available)",
        descriptor.operations.len(),
        descriptor.operations.len() - off.len()
    );
    for operation in off {
        let why = operation.reason.as_deref().unwrap_or("no reason given");
        let _ = writeln!(text, "  {} unavailable: {why}", operation.name);
    }
    for stream in &descriptor.streams {
        let _ = writeln!(text, "stream {} {}", stream.name, stream.version);
    }
    if !descriptor.features.is_empty() {
        let _ = writeln!(text, "features {}", descriptor.features.join(", "));
    }
    text
}

#[cfg(test)]
mod tests;
