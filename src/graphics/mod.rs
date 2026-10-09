use serde::{Deserialize, Serialize};

pub mod decode;
pub mod detect;
pub mod encode;
pub mod iterm;
pub mod kitty;
pub mod sixel;
pub mod tmux;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Support {
    pub protocol: Option<Protocol>,
    pub missing: Option<Missing>,
    pub cell: Option<CellSize>,
    pub tmux: Tmux,
    pub id_hi: u8,
}

pub const KITTY_LO: [u8; 2] = [0xf0, 0xf1];
const CAN: &str = "Ghostty, kitty, WezTerm, iTerm2, Konsole and foot can show them";
const PASSTHROUGH: &str = "add set -g allow-passthrough on to ~/.tmux.conf (or ~/.config/tmux/tmux.conf), run tmux \
     source-file on that file, then start cornercase again";
const ALL: &str = "all instead of on (tmux 3.4 or later) also lets images arrive while this window is hidden";

impl Support {
    pub fn images(&self) -> &'static str {
        self.protocol.map_or("none", Protocol::id)
    }

    pub fn cell_size(&self) -> String {
        self.cell.map_or_else(|| "unknown".into(), |cell| cell.to_string())
    }

    pub fn why_not(&self) -> &'static str {
        self.missing.as_ref().map_or("-", Missing::id)
    }

    pub fn forget(&self) -> Vec<u8> {
        if self.protocol != Some(Protocol::Kitty) {
            return Vec::new();
        }
        KITTY_LO
            .iter()
            .flat_map(|&lo| {
                let delete = kitty::delete(kitty::id(self.id_hi, lo));
                if self.tmux == Tmux::Wrap { tmux::wrap(&delete) } else { delete }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    Kitty,
    Iterm,
    Sixel { registers: u16, max: Option<(u32, u32)> },
}

impl Protocol {
    pub fn id(self) -> &'static str {
        match self {
            Self::Kitty => "kitty",
            Self::Iterm => "iterm",
            Self::Sixel { .. } => "sixel",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tmux {
    #[default]
    None,
    Wrap,
}

impl Tmux {
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Wrap => "wrap",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellSize {
    pub width: u16,
    pub height: u16,
}

impl std::fmt::Display for CellSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Missing {
    Off,
    Unknown { name: Option<String> },
    Cannot { name: String },
    VsCode,
    Tabby,
    Xterm,
    Foot,
    OldKonsole { version: String },
    OldKitty { version: String },
    GhosttyStorage,
    WezTermPixels,
    NoCellSize,
    TmuxPassthrough,
    TmuxSilent,
    TmuxOuter { name: Option<String> },
    TmuxNoSixel { version: Option<String> },
    TmuxNested,
    Zellij,
    OldZellij { version: Option<String> },
    Screen,
    Multiplexer { term: String },
}

impl Missing {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Unknown { .. } => "unknown",
            Self::Cannot { .. } => "cannot",
            Self::VsCode => "vscode",
            Self::Tabby => "tabby",
            Self::Xterm => "xterm",
            Self::Foot => "foot",
            Self::OldKonsole { .. } => "old-konsole",
            Self::OldKitty { .. } => "old-kitty",
            Self::GhosttyStorage => "ghostty-storage",
            Self::WezTermPixels => "wezterm-pixels",
            Self::NoCellSize => "no-cell-size",
            Self::TmuxPassthrough => "tmux-passthrough",
            Self::TmuxSilent => "tmux-silent",
            Self::TmuxOuter { .. } => "tmux-outer",
            Self::TmuxNoSixel { .. } => "tmux-no-sixel",
            Self::TmuxNested => "tmux-nested",
            Self::Zellij => "zellij",
            Self::OldZellij { .. } => "old-zellij",
            Self::Screen => "screen",
            Self::Multiplexer { .. } => "multiplexer",
        }
    }

    pub fn lines(&self) -> Vec<String> {
        match self {
            Self::Off => vec![
                "images are turned off in this window".into(),
                "CORNERCASE_IMAGES=off was set when cornercase started: start it without it to see them".into(),
            ],
            Self::Unknown { name } => vec![
                name.as_ref().map_or_else(
                    || "this terminal cannot show images, or cornercase could not tell which terminal it is".into(),
                    |name| format!("cornercase does not know how to show images in {name}"),
                ),
                CAN.into(),
                "if yours can, start cornercase with CORNERCASE_IMAGES=kitty, iterm or sixel".into(),
            ],
            Self::Cannot { name } => vec![format!("{name} cannot show images"), CAN.into()],
            Self::VsCode => vec![
                "VS Code's terminal shows images only with terminal.integrated.enableImages on".into(),
                "turn it on in Settings (it also needs terminal.integrated.gpuAcceleration), open a new terminal \
                 and start cornercase again"
                    .into(),
            ],
            Self::Tabby => vec![
                "Tabby shows images only with its Sixel setting on".into(),
                "turn on Settings › Terminal › Sixel, then start cornercase again".into(),
            ],
            Self::Xterm => vec![
                "xterm shows images only when it runs as a VT340".into(),
                "start it with xterm -ti vt340, or add XTerm*decTerminalID: vt340 to ~/.Xresources and run \
                 xrdb -merge ~/.Xresources"
                    .into(),
            ],
            Self::Foot => vec![
                "sixel images are turned off in foot".into(),
                "remove sixel=no from the [tweak] section of foot.ini and restart foot".into(),
            ],
            Self::OldKonsole { version } => {
                vec![format!("Konsole {version} cannot show images"), "update Konsole to 22.04 or later".into()]
            }
            Self::OldKitty { version } => {
                vec![
                    format!("kitty {version} is too old for images in cornercase"),
                    "update kitty to 0.28 or later".into(),
                ]
            }
            Self::GhosttyStorage => vec![
                "Ghostty did not answer the image query: its image-storage-limit is 0".into(),
                "remove image-storage-limit from Ghostty's config (the default is 320000000), reload it \
                 (ctrl+shift+, or cmd+shift+, on macOS), then start cornercase again"
                    .into(),
            ],
            Self::WezTermPixels => vec![
                "WezTerm did not report the window's size in pixels, so it shows no images".into(),
                "run cornercase in a local WezTerm tab rather than through a WezTerm multiplexer domain".into(),
            ],
            Self::NoCellSize => vec![
                "the terminal did not report its cell size in pixels, which sixel images need".into(),
                "run cornercase directly in the terminal: mosh and some multiplexers hide the window's size in pixels"
                    .into(),
            ],
            Self::TmuxPassthrough
            | Self::TmuxSilent
            | Self::TmuxOuter { .. }
            | Self::TmuxNoSixel { .. }
            | Self::TmuxNested
            | Self::Zellij
            | Self::OldZellij { .. }
            | Self::Screen
            | Self::Multiplexer { .. } => self.multiplexer_lines(),
        }
    }

    fn multiplexer_lines(&self) -> Vec<String> {
        match self {
            Self::TmuxPassthrough => {
                vec!["images need tmux to pass them through".into(), PASSTHROUGH.into(), ALL.into()]
            }
            Self::TmuxSilent => vec![
                "inside tmux, images need set -g allow-passthrough on".into(),
                PASSTHROUGH.into(),
                "if it is on already, cornercase's pane was hidden or not the active one when it started: start it \
                 again there (all instead of on, tmux 3.4 or later, also covers a hidden window)"
                    .into(),
                "otherwise the terminal running tmux cannot show images".into(),
            ],
            Self::TmuxOuter { name } => vec![
                format!("cornercase cannot show images in {} through tmux", name.as_deref().unwrap_or("this terminal")),
                "Ghostty, kitty and Rio can; outside tmux, more terminals can".into(),
            ],
            Self::TmuxNoSixel { version } => vec![
                version.as_ref().map_or_else(
                    || "this tmux cannot draw sixel images".into(),
                    |version| format!("this tmux ({version}) cannot draw sixel images"),
                ),
                "tmux 3.4 or later built with sixel can, or run cornercase outside tmux".into(),
            ],
            Self::TmuxNested => vec![
                "images do not pass through tmux inside tmux".into(),
                "run cornercase in the outer tmux, or outside tmux".into(),
            ],
            Self::Zellij => vec![
                "cornercase shows images in Zellij only as sixel, and this terminal has no sixel".into(),
                "run cornercase outside Zellij, or in a terminal with sixel such as foot, WezTerm or Konsole".into(),
            ],
            Self::OldZellij { version } => vec![
                format!(
                    "{} claims sixel whatever the terminal can do, so cornercase cannot tell",
                    version.as_ref().map_or_else(|| "this Zellij".to_string(), |version| format!("Zellij {version}"))
                ),
                "update Zellij to 0.45 or later, or run cornercase outside it".into(),
            ],
            Self::Screen => vec![
                "GNU screen cannot pass images through".into(),
                "run cornercase outside screen, or in tmux with set -g allow-passthrough on".into(),
            ],
            Self::Multiplexer { term } => {
                vec![
                    format!("this multiplexer (TERM={term}) cannot pass images through"),
                    "run cornercase outside it".into(),
                ]
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub id: u64,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub rgba: image::RgbaImage,
    pub opaque: bool,
}
