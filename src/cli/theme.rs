use std::io::Read as _;

use super::args::theme::ThemeArgs;
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::theme::{Scheme, ThemeFrom};
use crate::control::{self, ThemeInfo};
use crate::error::{Error, Result};
use crate::host::theme::HostTheme;

pub(super) fn read(source: &str) -> Result<Box<HostTheme>> {
    let text = if source == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        text
    } else {
        std::fs::read_to_string(source).map_err(|e| Error::Control(format!("{source}: {e}")))?
    };
    parse(source, &text).map(Box::new)
}

fn parse(source: &str, text: &str) -> Result<HostTheme> {
    let theme: HostTheme = serde_json::from_str(text).map_err(|e| Error::Control(format!("{source}: {e}")))?;
    control::theme::check(&theme).map_err(|e| Error::Control(format!("{source}: {e}")))?;
    Ok(theme)
}

pub(super) fn run(args: &ThemeArgs) -> Result<()> {
    let theme = args.file.as_deref().map(read).transpose()?;
    let request = control::Theme { theme, clear: args.clear };
    let value = ask("theme", control::Command::Theme(request))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&describe(answer::<ThemeInfo>(value)?))
}

fn describe(info: ThemeInfo) -> String {
    let from = match info.from {
        ThemeFrom::Default => "default",
        ThemeFrom::Window => "window",
        ThemeFrom::Client => "client",
    };
    match info.scheme {
        Some(Scheme::Light) => format!("{from} colours, light\n"),
        Some(Scheme::Dark) => format!("{from} colours, dark\n"),
        None => format!("{from} colours\n"),
    }
}

#[cfg(test)]
mod tests;
