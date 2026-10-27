use std::path::PathBuf;

use clap::{Args, ValueEnum};

use super::Print;
use crate::cli::help::PANE_REQUIRED;
use crate::control::terminal::{InputEvent, Point};

#[derive(Debug, Args)]
pub(crate) struct InputArgs {
    #[arg(long, value_name = "ID", required_unless_present_any = ["popup", "follow"], help = INPUT_PANE)]
    pub(crate) pane: Option<u64>,
    #[arg(long, help = "Type into the plugin popup open in the window instead of a pane")]
    pub(crate) popup: bool,
    #[arg(long, help = INPUT_FOLLOW)]
    pub(crate) follow: bool,
    #[arg(required = true, value_name = "EVENT", value_parser = event, help = "One JSON event, or - to read one per line from standard input")]
    pub(crate) events: Vec<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

const INPUT_PANE: &str = "The pane to type into; `cornercase status --panes` lists them. With --follow, for the lines that name \
     none. With --popup, the popup's id from its surface lines (`popup.pane`), so nothing reaches a popup opened since";

const INPUT_FOLLOW: &str =
    "Keep running and send each line of standard input as soon as it arrives, until it ends (give - as the only EVENT)";

fn event(text: &str) -> Result<String, String> {
    if text != "-" {
        serde_json::from_str::<InputEvent>(text).map_err(|e| format!("not an input event: {e}"))?;
    }
    Ok(text.to_string())
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("how").required(true).args(["up", "down", "top", "bottom"])))]
pub(crate) struct ScrollArgs {
    #[arg(long, value_name = "ID", required = true, help = PANE_REQUIRED)]
    pub(crate) pane: u64,
    #[arg(long, value_name = "ROWS", value_parser = rows, help = "Rows towards older output")]
    pub(crate) up: Option<i64>,
    #[arg(long, value_name = "ROWS", value_parser = rows, help = "Rows towards newer output")]
    pub(crate) down: Option<i64>,
    #[arg(long, help = "To the oldest line kept")]
    pub(crate) top: bool,
    #[arg(long, help = "Back to the live screen")]
    pub(crate) bottom: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

fn rows(text: &str) -> Result<i64, String> {
    text.parse::<i64>()
        .ok()
        .filter(|rows| (1..=i64::from(i32::MAX)).contains(rows))
        .ok_or_else(|| format!("`{text}` is not a number of rows from 1 up"))
}

#[derive(Debug, Args)]
pub(crate) struct SelectionArgs {
    #[arg(long, value_name = "ID", required = true, help = PANE_REQUIRED)]
    pub(crate) pane: u64,
    #[arg(long, value_name = "X,Y", value_parser = point, requires = "to", help = "Where the selection starts: column and row of the view, from 0")]
    pub(crate) from: Option<Point>,
    #[arg(long, value_name = "X,Y", value_parser = point, requires = "from", help = "Where the selection ends")]
    pub(crate) to: Option<Point>,
    #[arg(long, requires = "from", help = "Select a block of columns instead of whole lines")]
    pub(crate) rectangle: bool,
    #[arg(long, conflicts_with_all = ["from", "to", "copy"], help = "Remove the selection")]
    pub(crate) clear: bool,
    #[arg(long, help = "Also copy the selection to the clipboard, through the window")]
    pub(crate) copy: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

fn point(text: &str) -> Result<Point, String> {
    let wrong = || format!("`{text}` is not a cell as X,Y, such as 12,3");
    let (x, y) = text.split_once(',').ok_or_else(wrong)?;
    Ok(Point { x: x.trim().parse().map_err(|_| wrong())?, y: y.trim().parse().map_err(|_| wrong())? })
}

#[derive(Debug, Args)]
pub(crate) struct LinkArgs {
    #[arg(long, value_name = "ID", required = true, help = PANE_REQUIRED)]
    pub(crate) pane: u64,
    #[arg(long, value_name = "X,Y", required = true, value_parser = point, help = "The cell: column and row of the pane's view, from 0")]
    pub(crate) cell: Point,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PasteFileArgs {
    #[arg(long, value_name = "ID", required = true, help = PANE_REQUIRED)]
    pub(crate) pane: u64,
    #[arg(
        long,
        value_name = "NAME",
        help = "The file's name there [default: FILE's name, else pasted, with an image's extension]"
    )]
    pub(crate) name: Option<String>,
    #[arg(value_name = "FILE", help = "The file to paste, on this machine; - or none reads standard input")]
    pub(crate) file: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) print: Print,
}

const SURFACE_ID: &str = "The surface: the `surface` of the lines `cornercase surface` prints";

#[derive(Debug, Args)]
pub(crate) struct RedrawArgs {
    #[arg(long, value_name = "ID", required = true, help = SURFACE_ID)]
    pub(crate) surface: u64,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct ResizeArgs {
    #[arg(long, value_name = "ID", required = true, help = SURFACE_ID)]
    pub(crate) surface: u64,
    #[arg(long, value_name = "N", required = true, help = "Columns to lay the tab out in (1 to 1000)")]
    pub(crate) cols: u16,
    #[arg(long, value_name = "N", required = true, help = "Rows to lay the tab out in (1 to 500)")]
    pub(crate) rows: u16,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Side {
    First,
    Second,
}

#[derive(Debug, Args)]
pub(crate) struct RatioArgs {
    #[arg(long, value_name = "ID", required = true, help = "The tab; `cornercase status` lists them")]
    pub(crate) tab: u64,
    #[arg(
        long,
        value_delimiter = ',',
        value_enum,
        help = "The divider, as first or second at each level down [default: the outermost]"
    )]
    pub(crate) path: Vec<Side>,
    #[arg(value_name = "RATIO", value_parser = share, help = "The first side's share, between 0 and 1")]
    pub(crate) ratio: f64,
    #[command(flatten)]
    pub(crate) print: Print,
}

fn share(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0 && *ratio < 1.0)
        .ok_or_else(|| format!("`{text}` is not a share between 0 and 1, such as 0.5"))
}
