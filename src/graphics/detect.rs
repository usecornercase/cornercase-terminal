use super::{CellSize, Missing, Protocol, Support, Tmux};

pub const OVERRIDE_ENV: &str = "CORNERCASE_IMAGES";
pub const KITTY_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
pub const KITTY_QUERY_ID: &str = "31";
pub const TMUX_PROBE: &str = "\x1bPtmux;\x1b\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\x1b\\\x1b\\\
     \x1bPtmux;\x1b\x1b[>q\x1b\\\x1bPtmux;\x1b\x1b[c\x1b\\";
pub const TMUX_FORMAT: &str = "#{pane_tty}|#{allow-passthrough}|#{client_termtype}|#{client_termfeatures}";
pub const PANE_TITLE: &str = "\x1b]2;cornercase\x1b\\";
const DEFAULT_REGISTERS: u16 = 256;
const SIXEL: u32 = 4;
const MIN_KITTY: Version = (0, 28, 0);
const MIN_RIO: Version = (0, 5, 27);
const MIN_KONSOLE: u32 = 220_400;
const MIN_ZELLIJ: u32 = 4500;
const VTE_WITH_XTVERSION: u32 = 7600;
const VTE_WITHOUT_SIXEL: u32 = 8390;
const XTERM_JS_IMAGES: [u32; 4] = [62, 4, 9, 22];
const NAMES: [(&str, &str); 16] = [
    ("ghostty ", "Ghostty"),
    ("kitty(", "kitty"),
    ("rio ", "Rio"),
    ("iterm2 ", "iTerm2"),
    ("wezterm ", "WezTerm"),
    ("warp(", "Warp"),
    ("mintty ", "mintty"),
    ("xterm.js(", "xterm.js"),
    ("konsole ", "Konsole"),
    ("foot(", "foot"),
    ("xterm(", "xterm"),
    ("mlterm(", "mlterm"),
    ("contour ", "contour"),
    ("vte(", "VTE"),
    ("libvterm(", "libvterm"),
    ("zellij(", "Zellij"),
];

type Version = (u32, u32, u32);
type Var<'a> = &'a dyn Fn(&str) -> Option<String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replies {
    pub terminal: Option<String>,
    pub attributes: Option<Vec<u32>>,
    pub kitty: Option<String>,
    pub cell: Option<CellSize>,
    pub registers: Option<u32>,
    pub geometry: Option<(u32, u32)>,
}

impl Replies {
    fn has_sixel(&self) -> bool {
        self.attributes.as_ref().is_some_and(|attributes| attributes.contains(&SIXEL))
    }

    fn sixel(&self) -> Protocol {
        let registers =
            self.registers.filter(|&n| n > 0).map_or(DEFAULT_REGISTERS, |n| u16::try_from(n).unwrap_or(u16::MAX));
        Protocol::Sixel { registers, max: self.geometry }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outer {
    Local { passthrough: bool, terminal: Option<String>, sixel: bool },
    Probed(Replies),
    Stale,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub replies: Replies,
    pub asked_kitty: bool,
    pub window: Option<CellSize>,
    pub tmux: Option<Outer>,
}

impl Facts {
    pub fn cell(&self) -> Option<CellSize> {
        self.replies.cell.filter(|cell| plausible(*cell)).or(self.window.filter(|cell| plausible(*cell)))
    }

    pub fn resized(&mut self, window: Option<CellSize>) {
        if window.is_some() && window != self.window {
            self.replies.cell = None;
            self.window = window;
        }
    }

    pub fn checks_tmux_again(&self) -> bool {
        matches!(self.tmux, Some(Outer::Local { .. }))
    }

    pub fn renamed_tmux_pane(&self) -> bool {
        self.asked_kitty && self.replies.terminal.as_deref().is_some_and(is_tmux)
    }

    pub fn summary(&self) -> String {
        let cell = |cell: Option<CellSize>| cell.map_or_else(|| "-".to_string(), |c| c.to_string());
        format!(
            "da1={} kitty={} 16t={} window={} registers={} geometry={} tmux={}",
            attributes(self.replies.attributes.as_deref()),
            kitty_summary(self.asked_kitty, self.replies.kitty.as_deref()),
            cell(self.replies.cell),
            cell(self.window),
            self.replies.registers.map_or_else(|| "-".to_string(), |n| n.to_string()),
            self.replies.geometry.map_or_else(|| "-".to_string(), |(w, h)| format!("{w}x{h}")),
            match &self.tmux {
                None => "-".to_string(),
                Some(Outer::Local { passthrough, terminal, sixel }) => format!(
                    "local:{}:{}:{}",
                    if *passthrough { "on" } else { "off" },
                    terminal.as_deref().unwrap_or("-"),
                    if *sixel { "sixel" } else { "-" }
                ),
                Some(Outer::Probed(outer)) => format!(
                    "probed:{}:{}:{}",
                    outer.terminal.as_deref().unwrap_or("-"),
                    kitty_summary(true, outer.kitty.as_deref()),
                    attributes(outer.attributes.as_deref())
                ),
                Some(Outer::Stale) => "stale".to_string(),
            }
        )
    }
}

fn attributes(attributes: Option<&[u32]>) -> String {
    attributes.map_or_else(|| "-".to_string(), |a| a.iter().map(ToString::to_string).collect::<Vec<_>>().join(";"))
}

fn kitty_summary(asked: bool, reply: Option<&str>) -> String {
    match (asked, reply) {
        (false, _) => "not-asked".into(),
        (true, None) => "silent".into(),
        (true, Some(reply)) => reply.to_string(),
    }
}

pub fn window_cell(cols: u16, rows: u16, width: u16, height: u16) -> Option<CellSize> {
    (cols > 0 && rows > 0 && width > 0 && height > 0).then(|| CellSize { width: width / cols, height: height / rows })
}

fn plausible(cell: CellSize) -> bool {
    let (w, h) = (u32::from(cell.width), u32::from(cell.height));
    (2..=256).contains(&w) && (4..=512).contains(&h) && w <= h * 2 && h <= w * 8
}

pub fn overridden(var: impl Fn(&str) -> Option<String>) -> bool {
    var(OVERRIDE_ENV).is_some_and(|value| forced(&value, &Replies::default(), false).is_some())
}

pub fn ignored_override(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    var(OVERRIDE_ENV).filter(|value| forced(value, &Replies::default(), false).is_none())
}

pub fn in_multiplexer(var: impl Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let term = var("TERM").unwrap_or_default();
    set("TMUX") || set("STY") || set("ZELLIJ") || term.starts_with("tmux") || term.starts_with("screen")
}

pub fn likely_kitty(var: impl Fn(&str) -> Option<String>) -> bool {
    let is = |name: &str, values: &[&str]| var(name).is_some_and(|v| values.iter().any(|w| v.eq_ignore_ascii_case(w)));
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    is("TERM", &["xterm-kitty", "xterm-ghostty", "xterm-rio", "rio"])
        || is("TERM_PROGRAM", &["ghostty", "rio"])
        || set("KITTY_WINDOW_ID")
        || set("GHOSTTY_RESOURCES_DIR")
}

pub fn in_tmux(terminal: Option<&str>, var: impl Fn(&str) -> Option<String>) -> bool {
    multiplexer(terminal, &var, true) == Some(Mux::Tmux)
}

pub fn local_tmux(output: &str, own_tty: Option<&str>) -> Option<Outer> {
    let mut parts = output.lines().next()?.splitn(4, '|');
    let (tty, passthrough, terminal) = (parts.next()?.trim(), parts.next()?.trim(), parts.next()?.trim());
    let features = parts.next().unwrap_or_default();
    if own_tty.is_some_and(|own| !tty.is_empty() && own != tty) {
        return Some(Outer::Stale);
    }
    Some(Outer::Local {
        passthrough: !matches!(passthrough, "off" | "0"),
        terminal: (!terminal.is_empty()).then(|| terminal.to_string()),
        sixel: features.split(',').any(|feature| feature.trim() == "sixel"),
    })
}

pub fn decide(facts: &Facts, var: impl Fn(&str) -> Option<String>) -> Support {
    let mut support = Support { cell: facts.cell(), ..Support::default() };
    match choose(facts, &var) {
        Ok(pick) => {
            if matches!(pick.protocol, Protocol::Sixel { .. }) && support.cell.is_none() {
                support.missing = Some(Missing::NoCellSize);
            } else {
                support.protocol = Some(pick.protocol);
                support.tmux = pick.tmux;
            }
        }
        Err(missing) => support.missing = Some(missing),
    }
    support
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pick {
    protocol: Protocol,
    tmux: Tmux,
}

impl Pick {
    fn plain(protocol: Protocol) -> Self {
        Self { protocol, tmux: Tmux::None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mux {
    Tmux,
    Zellij,
    Screen,
    Unnamed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kitty {
    NotAsked,
    Silent,
    Answered,
}

impl Kitty {
    fn of(asked: bool, reply: Option<&str>) -> Self {
        match (asked, reply) {
            (false, _) => Self::NotAsked,
            (true, Some("OK")) => Self::Answered,
            (true, _) => Self::Silent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Brand {
    Ghostty,
    Kitty(Option<Version>),
    Rio(Option<Version>),
    Iterm2,
    WezTerm,
    Warp,
    Mintty,
    XtermJs,
    Tabby,
    Hyper,
    Konsole(Option<u32>),
    Foot,
    Xterm,
    Mlterm,
    Contour,
    Vte { version: Option<u32>, name: Option<&'static str> },
    Without(String),
    Unknown,
}

impl Brand {
    fn answers_xtversion(&self) -> bool {
        match self {
            Self::Ghostty
            | Self::Kitty(_)
            | Self::Rio(_)
            | Self::WezTerm
            | Self::Warp
            | Self::Foot
            | Self::Xterm
            | Self::Mlterm
            | Self::Contour => true,
            Self::Konsole(version) => version.is_none_or(|v| v >= MIN_KONSOLE),
            Self::Vte { version, .. } => version.is_none_or(|v| v >= VTE_WITH_XTVERSION),
            Self::Iterm2
            | Self::Mintty
            | Self::XtermJs
            | Self::Tabby
            | Self::Hyper
            | Self::Without(_)
            | Self::Unknown => false,
        }
    }

    fn draws_sixel(&self) -> bool {
        matches!(self, Self::Konsole(_) | Self::WezTerm | Self::Iterm2 | Self::Contour)
    }

    fn places_kitty(&self) -> bool {
        match self {
            Self::Ghostty => true,
            Self::Kitty(version) => version.is_none_or(|v| v >= MIN_KITTY),
            Self::Rio(version) => version.is_some_and(|v| v >= MIN_RIO),
            _ => false,
        }
    }
}

fn choose(facts: &Facts, var: Var) -> Result<Pick, Missing> {
    let replies = &facts.replies;
    let terminal = replies.terminal.as_deref();
    let mux = multiplexer(terminal, var, facts.tmux != Some(Outer::Stale));
    if let Some(forced) = var(OVERRIDE_ENV).and_then(|value| forced(&value, replies, mux == Some(Mux::Tmux))) {
        return forced;
    }
    match mux {
        Some(Mux::Tmux) => through_tmux(facts),
        Some(Mux::Zellij) => zellij(replies),
        Some(Mux::Screen) => Err(Missing::Screen),
        Some(Mux::Unnamed) => Err(Missing::Multiplexer { term: var("TERM").unwrap_or_default() }),
        None => by_brand(brand(terminal, replies.attributes.as_deref(), var), facts),
    }
}

fn forced(value: &str, replies: &Replies, tmux: bool) -> Option<Result<Pick, Missing>> {
    let tmux = if tmux { Tmux::Wrap } else { Tmux::None };
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(Err(Missing::Off)),
        "kitty" => Some(Ok(Pick { protocol: Protocol::Kitty, tmux })),
        "iterm" | "iterm2" => Some(Ok(Pick { protocol: Protocol::Iterm, tmux })),
        "sixel" => Some(Ok(Pick::plain(replies.sixel()))),
        _ => None,
    }
}

fn multiplexer(terminal: Option<&str>, var: Var, tmux_is_ours: bool) -> Option<Mux> {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    match terminal.map(str::to_ascii_lowercase) {
        Some(t) if t.starts_with("tmux ") => Some(Mux::Tmux),
        Some(t) if t.starts_with("zellij(") => Some(Mux::Zellij),
        None if tmux_is_ours && set("TMUX") => Some(Mux::Tmux),
        None if set("ZELLIJ") => Some(Mux::Zellij),
        None if set("STY") => Some(Mux::Screen),
        None if var("TERM").is_some_and(|term| term.starts_with("screen")) => Some(Mux::Unnamed),
        Some(_) | None => None,
    }
}

fn zellij(replies: &Replies) -> Result<Pick, Missing> {
    let lower = replies.terminal.as_deref().map(str::to_ascii_lowercase);
    match lower.as_deref().and_then(|t| t.strip_prefix("zellij(")).and_then(number) {
        Some(v) if v >= MIN_ZELLIJ => sixel(replies).ok_or(Missing::Zellij),
        version => Err(Missing::OldZellij {
            version: version.map(|v| format!("{}.{}.{}", v / 10_000, v / 100 % 100, v % 100)),
        }),
    }
}

fn is_tmux(terminal: &str) -> bool {
    terminal.to_ascii_lowercase().starts_with("tmux")
}

fn sixel(replies: &Replies) -> Option<Pick> {
    replies.has_sixel().then(|| Pick::plain(replies.sixel()))
}

fn through_tmux(facts: &Facts) -> Result<Pick, Missing> {
    match &facts.tmux {
        None | Some(Outer::Stale) => Err(Missing::TmuxSilent),
        Some(Outer::Local { passthrough, terminal, sixel }) => {
            beyond_tmux(terminal.as_deref(), Kitty::NotAsked, *passthrough, *sixel, &facts.replies)
        }
        Some(Outer::Probed(outer)) if outer.attributes.is_none() => Err(Missing::TmuxSilent),
        Some(Outer::Probed(outer)) => beyond_tmux(
            outer.terminal.as_deref(),
            Kitty::of(true, outer.kitty.as_deref()),
            true,
            outer.has_sixel(),
            &facts.replies,
        ),
    }
}

fn beyond_tmux(
    outer: Option<&str>,
    kitty: Kitty,
    passthrough: bool,
    outer_sixel: bool,
    inner: &Replies,
) -> Result<Pick, Missing> {
    if outer.is_some_and(is_tmux) {
        return Err(Missing::TmuxNested);
    }
    let brand = outer.map_or(Brand::Unknown, by_xtversion);
    if brand.places_kitty() {
        return match (passthrough, &brand, kitty) {
            (false, ..) => Err(Missing::TmuxPassthrough),
            (true, Brand::Ghostty, Kitty::Silent) => Err(Missing::GhosttyStorage),
            (true, ..) => Ok(Pick { protocol: Protocol::Kitty, tmux: Tmux::Wrap }),
        };
    }
    if outer_sixel && let Some(pick) = sixel(inner) {
        return Ok(pick);
    }
    if (outer_sixel || brand.draws_sixel()) && !inner.has_sixel() {
        let version = inner.terminal.as_deref().and_then(|t| t.strip_prefix("tmux ")).map(str::to_string);
        return Err(Missing::TmuxNoSixel { version });
    }
    Err(Missing::TmuxOuter { name: outer.map(pretty) })
}

fn pretty(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let Some((prefix, name)) = NAMES.iter().find(|(prefix, _)| lower.starts_with(prefix)) else {
        return text.to_string();
    };
    let version: String =
        text[prefix.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')).collect();
    if version.is_empty() || matches!(*name, "WezTerm" | "Warp") {
        (*name).to_string()
    } else {
        format!("{name} {version}")
    }
}

fn brand(terminal: Option<&str>, attributes: Option<&[u32]>, var: Var) -> Brand {
    if let Some(terminal) = terminal {
        return match by_xtversion(terminal) {
            Brand::Vte { version, .. } => Brand::Vte { version, name: vte_name(var) },
            Brand::Without(name) if name.starts_with("libvterm") => Brand::Without(libvterm_name(var)),
            Brand::XtermJs => xterm_js(var),
            brand => brand,
        };
    }
    from_env(var, attributes.is_some())
        .or_else(|| attributes.filter(|a| *a == XTERM_JS_IMAGES).map(|_| xterm_js(var)))
        .unwrap_or(Brand::Unknown)
}

fn xterm_js(var: Var) -> Brand {
    match var("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase().as_str() {
        "tabby" => Brand::Tabby,
        "hyper" => Brand::Hyper,
        _ => Brand::XtermJs,
    }
}

fn by_xtversion(text: &str) -> Brand {
    let lower = text.to_ascii_lowercase();
    let rest = |prefix: &str| lower.strip_prefix(prefix);
    if lower.starts_with("ghostty ") {
        Brand::Ghostty
    } else if let Some(rest) = rest("kitty(") {
        Brand::Kitty(version(rest))
    } else if let Some(rest) = rest("rio ") {
        Brand::Rio(version(rest))
    } else if let Some(rest) = rest("konsole ") {
        Brand::Konsole(version(rest).map(|(major, minor, micro)| major * 10_000 + minor * 100 + micro))
    } else if let Some(rest) = rest("vte(") {
        Brand::Vte { version: number(rest), name: None }
    } else if lower.starts_with("libvterm(") {
        Brand::Without("libvterm".into())
    } else {
        [
            ("iterm2 ", Brand::Iterm2),
            ("wezterm ", Brand::WezTerm),
            ("warp(", Brand::Warp),
            ("mintty ", Brand::Mintty),
            ("xterm.js(", Brand::XtermJs),
            ("foot(", Brand::Foot),
            ("xterm(", Brand::Xterm),
            ("mlterm(", Brand::Mlterm),
            ("contour ", Brand::Contour),
        ]
        .into_iter()
        .find_map(|(prefix, brand)| lower.starts_with(prefix).then_some(brand))
        .unwrap_or(Brand::Unknown)
    }
}

fn from_env(var: Var, responsive: bool) -> Option<Brand> {
    let get = |name: &str| var(name).filter(|value| !value.is_empty());
    let vte = || get("VTE_VERSION").and_then(|v| v.parse().ok());
    let hints = [
        get("TERM_PROGRAM").and_then(|program| by_program(&program, get("TERM_PROGRAM_VERSION").as_deref(), vte())),
        get("LC_TERMINAL").filter(|t| t.eq_ignore_ascii_case("iterm2")).map(|_| Brand::Iterm2),
        get("KITTY_WINDOW_ID").map(|_| Brand::Kitty(None)),
        get("GHOSTTY_RESOURCES_DIR").map(|_| Brand::Ghostty),
        get("KONSOLE_VERSION").map(|v| Brand::Konsole(v.parse().ok())),
        get("PTYXIS_VERSION").map(|_| Brand::Vte { version: vte(), name: Some("Ptyxis") }),
        get("VTE_VERSION").map(|v| Brand::Vte { version: v.parse().ok(), name: None }),
        get("TERMINAL_EMULATOR").filter(|t| t.contains("JetBrains")).map(|_| without("the JetBrains terminal")),
        get("INSIDE_EMACS").filter(|t| t.contains("vterm")).map(|_| without("Emacs vterm")),
        get("ALACRITTY_WINDOW_ID").map(|_| without("Alacritty")),
        get("XTERM_VERSION").map(|_| Brand::Xterm),
        get("TERM").and_then(|term| by_term(&term)),
    ];
    hints.into_iter().flatten().find(|brand| !responsive || !brand.answers_xtversion())
}

fn without(name: &str) -> Brand {
    Brand::Without(name.into())
}

fn by_program(program: &str, version_text: Option<&str>, vte: Option<u32>) -> Option<Brand> {
    Some(match program.to_ascii_lowercase().as_str() {
        "ghostty" => Brand::Ghostty,
        "iterm.app" => Brand::Iterm2,
        "wezterm" => Brand::WezTerm,
        "warpterminal" => Brand::Warp,
        "mintty" => Brand::Mintty,
        "vscode" => Brand::XtermJs,
        "tabby" => Brand::Tabby,
        "hyper" => Brand::Hyper,
        "rio" => Brand::Rio(version_text.and_then(version)),
        "contour" => Brand::Contour,
        "apple_terminal" => without("Terminal.app"),
        "kgx" => Brand::Vte { version: vte, name: Some("GNOME Console") },
        "blackbox" => Brand::Vte { version: vte, name: Some("Black Box") },
        _ => return None,
    })
}

fn by_term(term: &str) -> Option<Brand> {
    Some(match term {
        "xterm-kitty" => Brand::Kitty(None),
        "xterm-ghostty" => Brand::Ghostty,
        "xterm-rio" | "rio" => Brand::Rio(None),
        "alacritty" => without("Alacritty"),
        "contour" => Brand::Contour,
        "mlterm" => Brand::Mlterm,
        "linux" => without("the Linux console"),
        t if t.starts_with("foot") => Brand::Foot,
        t if t == "st" || t.starts_with("st-") => without("st"),
        t if t.starts_with("rxvt-unicode") => without("urxvt"),
        _ => return None,
    })
}

fn vte_name(var: Var) -> Option<&'static str> {
    let program = var("TERM_PROGRAM").unwrap_or_default().to_ascii_lowercase();
    if var("PTYXIS_VERSION").is_some() {
        Some("Ptyxis")
    } else if program == "kgx" {
        Some("GNOME Console")
    } else if program == "blackbox" {
        Some("Black Box")
    } else {
        None
    }
}

fn libvterm_name(var: Var) -> String {
    if var("INSIDE_EMACS").is_some_and(|value| value.contains("vterm")) {
        "Emacs vterm".into()
    } else {
        "this libvterm terminal".into()
    }
}

fn by_brand(brand: Brand, facts: &Facts) -> Result<Pick, Missing> {
    let replies = &facts.replies;
    let kitty = Kitty::of(facts.asked_kitty, replies.kitty.as_deref());
    let iterm = Ok(Pick::plain(Protocol::Iterm));
    let iterm_or = |missing: Missing| if replies.has_sixel() { Ok(Pick::plain(Protocol::Iterm)) } else { Err(missing) };
    match brand {
        Brand::Ghostty if kitty == Kitty::Silent => Err(Missing::GhosttyStorage),
        Brand::Kitty(Some(v)) if v < MIN_KITTY => Err(Missing::OldKitty { version: dotted(v) }),
        Brand::Rio(_) if kitty == Kitty::Silent => iterm,
        brand if brand.places_kitty() => Ok(Pick::plain(Protocol::Kitty)),
        Brand::WezTerm if facts.cell().is_none() => Err(Missing::WezTermPixels),
        Brand::Rio(_) | Brand::Iterm2 | Brand::WezTerm | Brand::Warp | Brand::Mintty => iterm,
        Brand::XtermJs => iterm_or(Missing::VsCode),
        Brand::Tabby => iterm_or(Missing::Tabby),
        Brand::Hyper => iterm_or(Missing::Cannot { name: "Hyper".into() }),
        Brand::Konsole(Some(v)) if v < MIN_KONSOLE => Err(Missing::OldKonsole { version: konsole(v) }),
        Brand::Konsole(_) | Brand::Contour => Ok(Pick::plain(replies.sixel())),
        Brand::Mlterm => sixel(replies).ok_or_else(|| Missing::Cannot { name: "mlterm".into() }),
        Brand::Foot => sixel(replies).ok_or(Missing::Foot),
        Brand::Xterm => sixel(replies).ok_or(Missing::Xterm),
        Brand::Vte { version: Some(v), .. } if v < VTE_WITHOUT_SIXEL && replies.has_sixel() => {
            Ok(Pick::plain(replies.sixel()))
        }
        Brand::Vte { version, name } => Err(Missing::Cannot { name: vte_label(version, name) }),
        Brand::Without(name) => Err(Missing::Cannot { name }),
        Brand::Ghostty | Brand::Kitty(_) | Brand::Unknown => {
            sixel(replies).ok_or_else(|| Missing::Unknown { name: replies.terminal.as_deref().map(pretty) })
        }
    }
}

fn vte_label(version: Option<u32>, name: Option<&str>) -> String {
    let vte = version.map(|v| format!("VTE {}.{}.{}", v / 10_000, v / 100 % 100, v % 100));
    match (name, vte) {
        (Some(name), Some(vte)) => format!("{name} ({vte})"),
        (Some(name), None) => name.to_string(),
        (None, Some(vte)) => vte,
        (None, None) => "this VTE terminal".into(),
    }
}

fn version(text: &str) -> Option<Version> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let mut parts = digits.split('.').map(|part| part.parse::<u32>().ok());
    let major = parts.next().flatten()?;
    Some((major, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

fn number(text: &str) -> Option<u32> {
    text.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

fn dotted((major, minor, micro): Version) -> String {
    format!("{major}.{minor}.{micro}")
}

fn konsole(version: u32) -> String {
    format!("{}.{:02}", version / 10_000, version / 100 % 100)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::host_theme::ThemeProbe;

    type Vars = &'static [(&'static str, &'static str)];

    struct Bench {
        env: Vars,
        replies: &'static [u8],
        through_tmux: &'static [u8],
        window: (u16, u16, u16, u16),
    }

    const WINDOW: (u16, u16, u16, u16) = (100, 30, 1000, 570);
    const TMUX_ANSWERS: &[u8] = b"\x1b[6;19;10t\x1b[?1;0;1024S\x1b[?2;3;0S\x1bP>|tmux 3.6\x1b\\\x1b[?1;2;4c";
    const TMUX_ENV: Vars =
        &[("TERM", "tmux-256color"), ("TERM_PROGRAM", "tmux"), ("TMUX", "/tmp/tmux-1000/default,1,0")];
    const OVER_SSH_FROM_TMUX: Vars = &[("TERM", "tmux-256color")];

    const GHOSTTY: Bench = Bench {
        env: &[
            ("TERM", "xterm-ghostty"),
            ("TERM_PROGRAM", "ghostty"),
            ("TERM_PROGRAM_VERSION", "1.3.1"),
            ("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty"),
        ],
        replies: b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;10t\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c",
        through_tmux: b"\x1b_Gi=31;OK\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c",
        window: WINDOW,
    };
    const KITTY: Bench = Bench {
        env: &[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")],
        replies: b"\x1b_Gi=31;OK\x1b\\\x1b[6;19;10t\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c",
        through_tmux: b"\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.45.0)\x1b\\\x1b[?62;52;c",
        window: WINDOW,
    };
    const WEZTERM: Bench = Bench {
        env: &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "WezTerm")],
        replies: b"\x1b[6;19;10t\x1b[?1;0;65536S\x1b[?2;0;1000;570S\x1bP>|WezTerm 20240203-110809-5046fc22\x1b\\\
            \x1b[?65;4;6;18;22c",
        through_tmux: b"\x1bP>|WezTerm 20240203-110809-5046fc22\x1b\\\x1b[?65;4;6;18;22c",
        window: WINDOW,
    };
    const WEZTERM_NIGHTLY_WITH_KITTY: Bench = Bench {
        env: &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "WezTerm")],
        replies: b"\x1b[6;19;10t\x1b[?1;0;65536S\x1b[?2;0;1000;570S\x1bP>|WezTerm 20261005-054844-37254829\x1b\\\
            \x1b[?65;4;6;18;22;52c",
        through_tmux: b"\x1b_Gi=31;OK\x1b\\\x1bP>|WezTerm 20261005-054844-37254829\x1b\\\x1b[?65;4;6;18;22;52c",
        window: WINDOW,
    };
    const KONSOLE: Bench = Bench {
        env: &[("TERM", "xterm-256color"), ("KONSOLE_VERSION", "251203")],
        replies: b"\x1b[6;19;10t\x1b[?1;0;256S\x1b[?2;0;16384;16384S\x1bP>|Konsole 25.12.3\x1b\\\x1b[?62;1;4c",
        through_tmux: b"\x1b_Gi=31;OK\x1b\\\x1bP>|Konsole 25.12.3\x1b\\\x1b[?62;1;4c",
        window: (100, 30, 1004, 570),
    };
    const FOOT: Bench = Bench {
        env: &[("TERM", "foot")],
        replies: b"\x1b[6;19;10t\x1b[?1;0;1024S\x1b[?2;0;1000;570S\x1bP>|foot(1.25.0)\x1b\\\x1b[?62;4;22;28;52c",
        through_tmux: b"\x1bP>|foot(1.25.0)\x1b\\\x1b[?62;4;22;28;52c",
        window: WINDOW,
    };
    const XTERM: Bench = Bench {
        env: &[("TERM", "xterm"), ("XTERM_VERSION", "XTerm(407)")],
        replies: b"\x1b[?1;3S\x1b[?2;3S\x1bP>|XTerm(407)\x1b\\\x1b[?64;1;2;6;9;15;17;18;21;22;28c",
        through_tmux: b"\x1bP>|XTerm(407)\x1b\\\x1b[?64;1;2;6;9;15;17;18;21;22;28c",
        window: (100, 30, 1000, 600),
    };
    const XTERM_VT340: Bench = Bench {
        env: &[("TERM", "xterm"), ("XTERM_VERSION", "XTerm(407)")],
        replies: b"\x1b[?1;0;1024S\x1b[?2;0;1000;600S\x1bP>|XTerm(407)\x1b\\\x1b[?63;1;2;4;6;9;15;17;22;28c",
        through_tmux: b"\x1bP>|XTerm(407)\x1b\\\x1b[?63;1;2;4;6;9;15;17;22;28c",
        window: (100, 30, 1000, 600),
    };

    const CELL: Option<CellSize> = Some(CellSize { width: 10, height: 19 });
    const XTERM_CELL: Option<CellSize> = Some(CellSize { width: 10, height: 20 });
    const TMUX_SIXEL: Protocol = Protocol::Sixel { registers: 1024, max: None };

    fn vars(vars: Vars) -> impl Fn(&str) -> Option<String> {
        move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_string())
    }

    fn heard(bytes: &[u8]) -> Replies {
        let mut probe = ThemeProbe::default();
        probe.feed(bytes);
        probe.finish().1
    }

    fn facts(env: Vars, replies: &[u8], window: (u16, u16, u16, u16)) -> Facts {
        let var = vars(env);
        Facts {
            replies: heard(replies),
            asked_kitty: !in_multiplexer(&var) && likely_kitty(&var),
            window: window_cell(window.0, window.1, window.2, window.3),
            tmux: None,
        }
    }

    fn direct(bench: &Bench) -> Support {
        decide(&facts(bench.env, bench.replies, bench.window), vars(bench.env))
    }

    const OWN_TTY: &str = "/dev/pts/7";

    fn local_tmux_with(bench: &Bench, passthrough: &str) -> Support {
        local_tmux_built(bench, passthrough, "tmux 3.6", "1;2;4")
    }

    fn local_tmux_built(bench: &Bench, passthrough: &str, tmux: &str, attributes: &str) -> Support {
        let outer = heard(bench.replies);
        let features = if outer.has_sixel() { "256,RGB,bpaste,sixel,sync" } else { "256,RGB,bpaste,sync" };
        let output = format!("{OWN_TTY}|{passthrough}|{}|{features}\n", outer.terminal.as_deref().unwrap_or_default());
        let (cols, rows, width, height) = bench.window;
        let answers = format!(
            "\x1b[6;{};{}t\x1b[?1;0;1024S\x1b[?2;3;0S\x1bP>|{tmux}\x1b\\\x1b[?{attributes}c",
            height / rows,
            width / cols
        );
        let mut facts = facts(TMUX_ENV, answers.as_bytes(), bench.window);
        facts.tmux = local_tmux(&output, Some(OWN_TTY));
        decide(&facts, vars(TMUX_ENV))
    }

    fn remote_tmux_with(through_tmux: &[u8]) -> Support {
        let mut facts = facts(OVER_SSH_FROM_TMUX, TMUX_ANSWERS, WINDOW);
        facts.tmux = Some(Outer::Probed(heard(through_tmux)));
        decide(&facts, vars(OVER_SSH_FROM_TMUX))
    }

    fn shows(protocol: Protocol, cell: Option<CellSize>, tmux: Tmux) -> Support {
        Support { protocol: Some(protocol), missing: None, cell, tmux, id_hi: 0 }
    }

    fn says(missing: Missing, cell: Option<CellSize>) -> Support {
        Support { protocol: None, missing: Some(missing), cell, tmux: Tmux::None, id_hi: 0 }
    }

    mod bench {
        use super::*;

        #[rstest]
        #[case::ghostty_1_3_1(GHOSTTY, shows(Protocol::Kitty, CELL, Tmux::None))]
        #[case::kitty_0_45(KITTY, shows(Protocol::Kitty, CELL, Tmux::None))]
        #[case::wezterm_20240203(WEZTERM, shows(Protocol::Iterm, CELL, Tmux::None))]
        #[case::wezterm_nightly_with_kitty_graphics_on(
            WEZTERM_NIGHTLY_WITH_KITTY,
            shows(Protocol::Iterm, CELL, Tmux::None)
        )]
        #[case::konsole_25_12(KONSOLE, shows(Protocol::Sixel { registers: 256, max: Some((16384, 16384)) }, CELL, Tmux::None))]
        #[case::foot_1_25(FOOT, shows(Protocol::Sixel { registers: 1024, max: Some((1000, 570)) }, CELL, Tmux::None))]
        #[case::xterm_407(XTERM, says(Missing::Xterm, XTERM_CELL))]
        #[case::xterm_407_as_a_vt340(XTERM_VT340, shows(Protocol::Sixel { registers: 1024, max: Some((1000, 600)) }, XTERM_CELL, Tmux::None))]
        fn picks_what_each_terminal_shows(#[case] bench: Bench, #[case] expected: Support) {
            assert_eq!(direct(&bench), expected);
        }

        #[rstest]
        #[case::ghostty(GHOSTTY, shows(Protocol::Kitty, CELL, Tmux::Wrap))]
        #[case::kitty(KITTY, shows(Protocol::Kitty, CELL, Tmux::Wrap))]
        #[case::wezterm(WEZTERM, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::konsole(KONSOLE, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::foot(FOOT, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::xterm_as_a_vt340(XTERM_VT340, shows(TMUX_SIXEL, XTERM_CELL, Tmux::None))]
        #[case::xterm(XTERM, says(Missing::TmuxOuter { name: Some("xterm 407".into()) }, XTERM_CELL))]
        fn passes_kitty_placeholders_through_tmux_or_lets_tmux_draw_sixel(
            #[case] bench: Bench,
            #[case] expected: Support,
        ) {
            assert_eq!(local_tmux_with(&bench, "on"), expected);
        }

        #[rstest]
        #[case::ghostty(GHOSTTY, says(Missing::TmuxPassthrough, CELL))]
        #[case::kitty(KITTY, says(Missing::TmuxPassthrough, CELL))]
        #[case::foot(FOOT, shows(TMUX_SIXEL, CELL, Tmux::None))]
        fn asks_for_passthrough_only_where_it_is_needed(#[case] bench: Bench, #[case] expected: Support) {
            assert_eq!(local_tmux_with(&bench, "off"), expected);
        }

        #[rstest]
        #[case::ghostty(GHOSTTY, shows(Protocol::Kitty, CELL, Tmux::Wrap))]
        #[case::kitty(KITTY, shows(Protocol::Kitty, CELL, Tmux::Wrap))]
        #[case::wezterm(WEZTERM, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::wezterm_nightly_with_kitty_graphics_on(WEZTERM_NIGHTLY_WITH_KITTY, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::konsole(KONSOLE, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::foot(FOOT, shows(TMUX_SIXEL, CELL, Tmux::None))]
        #[case::xterm(XTERM, says(Missing::TmuxOuter { name: Some("xterm 407".into()) }, CELL))]
        fn asks_the_terminal_outside_a_tmux_it_reaches_over_ssh(#[case] bench: Bench, #[case] expected: Support) {
            assert_eq!(remote_tmux_with(bench.through_tmux), expected);
        }

        #[rstest]
        #[case::foot(FOOT)]
        #[case::wezterm(WEZTERM)]
        #[case::konsole(KONSOLE)]
        #[case::xterm_as_a_vt340(XTERM_VT340)]
        fn says_a_tmux_without_sixel_cannot_draw_it_for_a_sixel_terminal(#[case] bench: Bench) {
            let support = local_tmux_built(&bench, "on", "tmux 3.3a", "1;2");

            assert_eq!(support.missing, Some(Missing::TmuxNoSixel { version: Some("3.3a".into()) }));
        }

        #[test]
        fn says_so_over_ssh_too() {
            let mut facts = facts(OVER_SSH_FROM_TMUX, b"\x1b[6;19;10t\x1bP>|tmux 3.3a\x1b\\\x1b[?1;2c", WINDOW);
            facts.tmux = Some(Outer::Probed(heard(FOOT.through_tmux)));

            assert_eq!(
                decide(&facts, vars(OVER_SSH_FROM_TMUX)),
                says(Missing::TmuxNoSixel { version: Some("3.3a".into()) }, CELL)
            );
        }

        #[test]
        fn hears_nothing_through_tmux_with_passthrough_off() {
            assert_eq!(remote_tmux_with(b""), says(Missing::TmuxSilent, CELL));
        }

        #[test]
        fn never_asks_the_kitty_query_inside_tmux() {
            assert!(!facts(TMUX_ENV, TMUX_ANSWERS, WINDOW).asked_kitty);
        }

        #[rstest]
        #[case::ghostty(GHOSTTY, true)]
        #[case::kitty(KITTY, true)]
        #[case::wezterm(WEZTERM, false)]
        #[case::konsole(KONSOLE, false)]
        #[case::foot(FOOT, false)]
        #[case::xterm(XTERM, false)]
        fn asks_the_kitty_query_only_where_kitty_placeholders_are_likely(#[case] bench: Bench, #[case] asked: bool) {
            assert_eq!(facts(bench.env, bench.replies, bench.window).asked_kitty, asked);
        }

        #[test]
        fn summarises_what_ghostty_answered_for_the_log() {
            assert_eq!(
                facts(GHOSTTY.env, GHOSTTY.replies, GHOSTTY.window).summary(),
                "da1=62;22;52 kitty=OK 16t=10x19 window=10x19 registers=- geometry=- tmux=-"
            );
        }
    }

    mod terminals {
        use super::*;

        fn decided(env: Vars, replies: &[u8]) -> Support {
            decide(&facts(env, replies, WINDOW), vars(env))
        }

        #[rstest]
        #[case::iterm2(&[("LC_TERMINAL", "iTerm2")], b"\x1bP>|iTerm2 3.6.5\x1b\\\x1b[?64;1;2;4;6;17;18;21;22;52c", Protocol::Iterm)]
        #[case::warp(&[("TERM_PROGRAM", "WarpTerminal")], b"\x1bP>|Warp(v0.2026.06.03.09.49.stable_01)\x1b\\\x1b[?62c", Protocol::Iterm)]
        #[case::mintty(&[], b"\x1bP>|mintty 3.8.3\x1b\\\x1b[?64;2;3;4;6;9;11;15;21;28;29;1;22c", Protocol::Iterm)]
        #[case::rio_with_placeholders(&[("TERM", "xterm-rio")], b"\x1b_Gi=31;OK\x1b\\\x1bP>|Rio 0.5.28\x1b\\\x1b[?62;4;6;22;52c", Protocol::Kitty)]
        #[case::rio_answering_an_error(&[("TERM", "xterm-rio")], b"\x1b_Gi=31;EINVAL:bad\x1b\\\x1bP>|Rio 0.5.28\x1b\\\x1b[?62;4;6;22;52c", Protocol::Iterm)]
        #[case::zellij_0_45_on_a_terminal_with_sixel(&[], b"\x1b[?1;0;256S\x1bP>|Zellij(4500)\x1b\\\x1b[?62;4;52c", Protocol::Sixel { registers: 256, max: None })]
        #[case::mlterm_with_images(&[], b"\x1bP>|mlterm(3.9.4)\x1b\\\x1b[?63;1;2;3;4;6;9;15;18;22;29c", Protocol::Sixel { registers: 256, max: None })]
        #[case::rio_before_placeholders_worked(&[("TERM", "xterm-rio")], b"\x1b_Gi=31;OK\x1b\\\x1bP>|Rio 0.5.26\x1b\\\x1b[?62;4;6;22;52c", Protocol::Iterm)]
        #[case::ghostty_over_ssh_without_its_terminfo(&[("TERM", "xterm-256color")], b"\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", Protocol::Kitty)]
        #[case::vs_code_with_images_on(&[("TERM_PROGRAM", "vscode")], b"\x1bP>|xterm.js(6.1.0-beta.91)\x1b\\\x1b[?62;4;9;22c", Protocol::Iterm)]
        #[case::tabby(&[("TERM_PROGRAM", "Tabby")], b"\x1b[?62;4;9;22c", Protocol::Iterm)]
        #[case::contour(&[], b"\x1bP>|contour 0.7.1\x1b\\\x1b[?65;1;2;3;4;6;52c", Protocol::Sixel { registers: 256, max: None })]
        #[case::an_unknown_terminal_with_sixel(&[], b"\x1bP>|Bobcat 1.0\x1b\\\x1b[?62;4c", Protocol::Sixel { registers: 256, max: None })]
        #[case::old_vte_with_sixel_on(&[("VTE_VERSION", "7801")], b"\x1bP>|VTE(7801)\x1b\\\x1b[?61;1;4;21;22;28c", Protocol::Sixel { registers: 256, max: None })]
        #[case::zellij_on_a_terminal_with_sixel(&[], b"\x1b[?1;0;1024S\x1bP>|Zellij(4501)\x1b\\\x1b[?62;4;52c", Protocol::Sixel { registers: 1024, max: None })]
        #[case::a_stale_tmux_variable(&[("TMUX", "/tmp/tmux-1000/default,1,0"), ("TERM", "xterm-ghostty")], b"\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", Protocol::Kitty)]
        fn shows_images_in(#[case] env: Vars, #[case] replies: &[u8], #[case] protocol: Protocol) {
            assert_eq!(decided(env, replies), shows(protocol, CELL, Tmux::None));
        }

        #[rstest]
        #[case::ghostty_with_image_storage_limit_0(GHOSTTY.env, b"\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", Missing::GhosttyStorage)]
        #[case::kitty_0_27(&[("TERM", "xterm-kitty")], b"\x1bP>|kitty(0.27.0)\x1b\\\x1b[?62;52;c", Missing::OldKitty { version: "0.27.0".into() })]
        #[case::vs_code_with_images_off(&[("TERM_PROGRAM", "vscode")], b"\x1bP>|xterm.js(6.1.0-beta.91)\x1b\\\x1b[?1;2c", Missing::VsCode)]
        #[case::tabby_with_sixel_off(&[("TERM_PROGRAM", "Tabby")], b"\x1b[?62;9;22c", Missing::Tabby)]
        #[case::hyper_3(&[("TERM_PROGRAM", "Hyper")], b"\x1b[?1;2c", Missing::Cannot { name: "Hyper".into() })]
        #[case::foot_with_sixel_off(&[("TERM", "foot")], b"\x1bP>|foot(1.25.0)\x1b\\\x1b[?62;22;28;52c", Missing::Foot)]
        #[case::konsole_21_12(&[("KONSOLE_VERSION", "211208")], b"\x1b[?62;1;4c", Missing::OldKonsole { version: "21.12".into() })]
        #[case::gnome_console(&[("TERM_PROGRAM", "kgx"), ("VTE_VERSION", "8401")], b"\x1bP>|VTE(8401)\x1b\\\x1b[?61;1;4;21;22;28c", Missing::Cannot { name: "GNOME Console (VTE 0.84.1)".into() })]
        #[case::ptyxis(&[("PTYXIS_VERSION", "50.1"), ("VTE_VERSION", "8401")], b"\x1bP>|VTE(8401)\x1b\\\x1b[?61;1;21;22;28c", Missing::Cannot { name: "Ptyxis (VTE 0.84.1)".into() })]
        #[case::alacritty(&[("TERM", "alacritty")], b"\x1b[?6c", Missing::Cannot { name: "Alacritty".into() })]
        #[case::alacritty_started_from_ghostty(&[("TERM", "alacritty"), ("TERM_PROGRAM", "ghostty")], b"\x1b[?6c", Missing::Cannot { name: "Alacritty".into() })]
        #[case::terminal_app(&[("TERM_PROGRAM", "Apple_Terminal")], b"\x1b[?1;2c", Missing::Cannot { name: "Terminal.app".into() })]
        #[case::st(&[("TERM", "st-256color")], b"\x1b[?6c", Missing::Cannot { name: "st".into() })]
        #[case::urxvt(&[("TERM", "rxvt-unicode-256color")], b"\x1b[?1;2c", Missing::Cannot { name: "urxvt".into() })]
        #[case::jetbrains(&[("TERMINAL_EMULATOR", "JetBrains-JediTerm")], b"\x1b[?6c", Missing::Cannot { name: "the JetBrains terminal".into() })]
        #[case::emacs_vterm(&[("INSIDE_EMACS", "30.1,vterm")], b"\x1bP>|libvterm(0.3)\x1b\\\x1b[?1;2c", Missing::Cannot { name: "Emacs vterm".into() })]
        #[case::connectbot(&[], b"\x1bP>|libvterm(0.3)\x1b\\\x1b[?1;2c", Missing::Cannot { name: "this libvterm terminal".into() })]
        #[case::the_linux_console(&[("TERM", "linux")], b"\x1b[?6c", Missing::Cannot { name: "the Linux console".into() })]
        #[case::a_terminal_that_says_nothing(&[("TERM", "xterm-256color")], b"\x1b[?1;2c", Missing::Unknown { name: None })]
        #[case::an_unknown_terminal_without_sixel(&[], b"\x1bP>|Bobcat 1.0\x1b\\\x1b[?62c", Missing::Unknown { name: Some("Bobcat 1.0".into()) })]
        #[case::zellij_on_a_terminal_without_sixel(&[("ZELLIJ", "0")], b"\x1bP>|Zellij(4501)\x1b\\\x1b[?62;52c", Missing::Zellij)]
        #[case::gnu_screen(&[("STY", "1234.pts-0.host"), ("TERM", "screen-256color")], b"\x1b[?1;2c", Missing::Screen)]
        #[case::a_multiplexer_without_a_name(&[("TERM", "screen.xterm-256color")], b"\x1b[?1;2c", Missing::Multiplexer { term: "screen.xterm-256color".into() })]
        #[case::zellij_0_44(&[("ZELLIJ", "0")], b"\x1bP>|Zellij(4401)\x1b\\\x1b[?62;4;52c", Missing::OldZellij { version: Some("0.44.1".into()) })]
        #[case::zellij_before_xtversion(&[("ZELLIJ", "0")], b"\x1b[?62;4c", Missing::OldZellij { version: None })]
        #[case::ghostty_answering_an_error(GHOSTTY.env, b"\x1b_Gi=31;ENOSPC:no room\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", Missing::GhosttyStorage)]
        #[case::tabby_once_xterm_js_answers(&[("TERM_PROGRAM", "Tabby")], b"\x1bP>|xterm.js(6.1.0)\x1b\\\x1b[?62;9;22c", Missing::Tabby)]
        #[case::hyper_once_xterm_js_answers(&[("TERM_PROGRAM", "Hyper")], b"\x1bP>|xterm.js(6.1.0)\x1b\\\x1b[?1;2c", Missing::Cannot { name: "Hyper".into() })]
        #[case::mlterm_without_images(&[], b"\x1bP>|mlterm(3.9.4)\x1b\\\x1b[?63;1;2;3;6;9;15;18;22;29c", Missing::Cannot { name: "mlterm".into() })]
        #[case::an_unknown_name_as_the_terminal_says_it(&[], b"\x1bP>|Bobcat 1.0\x1b\\\x1b[?62c", Missing::Unknown { name: Some("Bobcat 1.0".into()) })]
        #[case::tmux_that_was_never_asked(TMUX_ENV, TMUX_ANSWERS, Missing::TmuxSilent)]
        fn says_why_there_are_no_images_in(#[case] env: Vars, #[case] replies: &[u8], #[case] missing: Missing) {
            assert_eq!(decided(env, replies), says(missing, CELL));
        }

        #[test]
        fn says_tmux_inside_tmux_cannot_pass_images() {
            let mut facts = facts(TMUX_ENV, TMUX_ANSWERS, WINDOW);
            facts.tmux = local_tmux("/dev/pts/7|on|tmux 3.6|256,RGB,sixel\n", Some("/dev/pts/7"));

            assert_eq!(decide(&facts, vars(TMUX_ENV)), says(Missing::TmuxNested, CELL));
        }

        #[test]
        fn says_ghostty_has_images_off_when_it_does_not_answer_through_tmux() {
            assert_eq!(
                remote_tmux_with(b"\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c"),
                says(Missing::GhosttyStorage, CELL)
            );
        }

        #[test]
        fn needs_a_cell_size_for_sixel() {
            let support = decide(
                &facts(FOOT.env, b"\x1bP>|foot(1.25.0)\x1b\\\x1b[?62;4;22;28;52c", (0, 0, 0, 0)),
                vars(FOOT.env),
            );

            assert_eq!(support, says(Missing::NoCellSize, None));
        }

        #[test]
        fn says_wezterm_needs_the_window_s_pixel_size() {
            let support = decide(
                &facts(WEZTERM.env, b"\x1bP>|WezTerm 20240203\x1b\\\x1b[?65;4;6;18;22c", (0, 0, 0, 0)),
                vars(WEZTERM.env),
            );

            assert_eq!(support, says(Missing::WezTermPixels, None));
        }

        #[test]
        fn recognises_xterm_js_images_over_ssh_without_a_cell_size() {
            let support = decide(&facts(&[("TERM", "xterm-256color")], b"\x1b[?62;4;9;22c", (0, 0, 0, 0)), vars(&[]));

            assert_eq!(support, shows(Protocol::Iterm, None, Tmux::None));
        }

        #[test]
        fn shows_kitty_and_iterm_images_without_a_cell_size() {
            let kitty = decide(
                &facts(GHOSTTY.env, b"\x1b_Gi=31;OK\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c", (0, 0, 0, 0)),
                vars(GHOSTTY.env),
            );

            assert_eq!(kitty, shows(Protocol::Kitty, None, Tmux::None));
        }
    }

    mod forced {
        use super::*;

        const ALACRITTY: &[u8] = b"\x1b[6;19;10t\x1b[?6c";

        fn forced_to(value: &'static str, base: Vars) -> Support {
            let env: Vec<(&str, &str)> = base.iter().copied().chain([(OVERRIDE_ENV, value)]).collect();
            let env: Vars = Box::leak(env.into_boxed_slice());
            decide(&facts(env, if base == TMUX_ENV { TMUX_ANSWERS } else { ALACRITTY }, WINDOW), vars(env))
        }

        #[rstest]
        #[case::kitty("kitty", shows(Protocol::Kitty, CELL, Tmux::None))]
        #[case::iterm("iterm", shows(Protocol::Iterm, CELL, Tmux::None))]
        #[case::iterm2("ITerm2", shows(Protocol::Iterm, CELL, Tmux::None))]
        #[case::sixel("sixel", shows(Protocol::Sixel { registers: 256, max: None }, CELL, Tmux::None))]
        #[case::off("off", says(Missing::Off, CELL))]
        #[case::a_typo("kity", says(Missing::Cannot { name: "Alacritty".into() }, CELL))]
        fn takes_the_protocol_from_cornercase_images(#[case] value: &'static str, #[case] expected: Support) {
            assert_eq!(forced_to(value, &[("TERM", "alacritty")]), expected);
        }

        #[test]
        fn wraps_a_forced_kitty_for_tmux() {
            assert_eq!(forced_to("kitty", TMUX_ENV), shows(Protocol::Kitty, CELL, Tmux::Wrap));
        }

        #[rstest]
        #[case::kitty("kitty", true)]
        #[case::off(" OFF ", true)]
        #[case::unknown("halfblocks", false)]
        fn knows_when_it_is_overridden(#[case] value: &'static str, #[case] expected: bool) {
            assert_eq!(overridden(|name| (name == OVERRIDE_ENV).then(|| value.to_string())), expected);
        }
    }

    mod environment {
        use super::*;

        #[rstest]
        #[case::tmux(&[("TMUX", "/tmp/tmux-1000/default,1,0")], true)]
        #[case::screen(&[("STY", "1.pts-0.host")], true)]
        #[case::zellij(&[("ZELLIJ", "0")], true)]
        #[case::tmux_over_ssh(&[("TERM", "tmux-256color")], true)]
        #[case::old_tmux_over_ssh(&[("TERM", "screen-256color")], true)]
        #[case::ghostty(GHOSTTY.env, false)]
        #[case::nothing(&[], false)]
        fn tells_a_multiplexer_before_asking(#[case] env: Vars, #[case] expected: bool) {
            assert_eq!(in_multiplexer(vars(env)), expected);
        }

        #[rstest]
        #[case::ghostty(&[("TERM", "xterm-ghostty")], true)]
        #[case::kitty_over_kitten_ssh(&[("TERM", "xterm-256color"), ("KITTY_WINDOW_ID", "3")], true)]
        #[case::rio(&[("TERM_PROGRAM", "rio")], true)]
        #[case::ghostty_s_resources(&[("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")], true)]
        #[case::plain_xterm(&[("TERM", "xterm-256color")], false)]
        #[case::wezterm(&[("TERM_PROGRAM", "WezTerm")], false)]
        fn tells_a_likely_kitty_placeholder_terminal(#[case] env: Vars, #[case] expected: bool) {
            assert_eq!(likely_kitty(vars(env)), expected);
        }

        #[rstest]
        #[case::a_typo("sixels", Some("sixels"))]
        #[case::valid("iterm", None)]
        fn keeps_an_unknown_override_for_the_log(#[case] value: &'static str, #[case] expected: Option<&str>) {
            let ignored = ignored_override(|name| (name == OVERRIDE_ENV).then(|| value.to_string()));

            assert_eq!(ignored.as_deref(), expected);
        }

        #[test]
        fn knows_it_renamed_a_tmux_pane_it_did_not_know_about() {
            let facts =
                Facts { replies: heard(b"\x1bP>|tmux 3.6\x1b\\\x1b[?1;2;4c"), asked_kitty: true, ..Facts::default() };

            assert!(facts.renamed_tmux_pane());
        }
    }

    mod tmux_answer {
        use super::*;

        fn ghostty(passthrough: bool) -> Outer {
            Outer::Local { passthrough, terminal: Some("ghostty 1.3.1".into()), sixel: false }
        }

        #[rstest]
        #[case::on(
            "/dev/pts/7|on|ghostty 1.3.1|bpaste,ccolour,clipboard,cstyle,focus,RGB,title\n",
            Some(ghostty(true))
        )]
        #[case::all("/dev/pts/7|all|ghostty 1.3.1|256,RGB\n", Some(ghostty(true)))]
        #[case::off("/dev/pts/7|off|ghostty 1.3.1|256,RGB\n", Some(ghostty(false)))]
        #[case::off_in_tmux_3_3("/dev/pts/7|0|ghostty 1.3.1|256,RGB\n", Some(ghostty(false)))]
        #[case::on_in_tmux_3_3("/dev/pts/7|1|ghostty 1.3.1|256,RGB\n", Some(ghostty(true)))]
        #[case::before_the_option_existed("/dev/pts/7||ghostty 1.3.1|256\n", Some(ghostty(true)))]
        #[case::off_with_sixel("/dev/pts/7|off|foot(1.25.0)|256,RGB,sixel,sync\n", Some(Outer::Local { passthrough: false, terminal: Some("foot(1.25.0)".into()), sixel: true }))]
        #[case::an_outer_terminal_without_a_name("/dev/pts/7|on||256\n", Some(Outer::Local { passthrough: true, terminal: None, sixel: false }))]
        #[case::another_tty("/dev/pts/3|on|ghostty 1.3.1|256\n", Some(Outer::Stale))]
        #[case::nothing("", None)]
        fn reads_what_tmux_display_message_prints(#[case] output: &str, #[case] expected: Option<Outer>) {
            assert_eq!(local_tmux(output, Some("/dev/pts/7")), expected);
        }

        #[test]
        fn trusts_a_pane_whose_tty_it_cannot_compare() {
            assert_eq!(local_tmux("/dev/pts/3|on|ghostty 1.3.1|256\n", None), Some(ghostty(true)));
        }

        #[test]
        fn ignores_a_tmux_variable_left_over_from_another_terminal() {
            let env: Vars = &[("TMUX", "/tmp/tmux-1000/default,1,0"), ("TMUX_PANE", "%3"), ("TERM", "alacritty")];
            let mut facts = facts(env, b"\x1b[?6c", WINDOW);
            facts.tmux = local_tmux("/dev/pts/3|on|ghostty 1.3.1|256\n", Some("/dev/pts/7"));

            assert_eq!(decide(&facts, vars(env)), says(Missing::Cannot { name: "Alacritty".into() }, CELL));
        }

        #[test]
        fn asks_tmux_again_only_when_it_answered_locally() {
            let local = Facts { tmux: Some(ghostty(true)), ..Facts::default() };
            let probed = Facts { tmux: Some(Outer::Probed(Replies::default())), ..Facts::default() };

            assert_eq!((local.checks_tmux_again(), probed.checks_tmux_again()), (true, false));
        }

        #[test]
        fn follows_another_terminal_attaching_to_the_session() {
            let mut facts = facts(TMUX_ENV, TMUX_ANSWERS, WINDOW);
            facts.tmux = Some(ghostty(true));
            let before = decide(&facts, vars(TMUX_ENV));
            facts.tmux = local_tmux("/dev/pts/7|on|XTerm(407)|256,RGB\n", Some("/dev/pts/7"));

            assert_eq!(
                (before.protocol, decide(&facts, vars(TMUX_ENV)).missing),
                (Some(Protocol::Kitty), Some(Missing::TmuxOuter { name: Some("xterm 407".into()) }))
            );
        }
    }

    mod cell {
        use super::*;

        const fn sized(width: u16, height: u16) -> CellSize {
            CellSize { width, height }
        }

        #[rstest]
        #[case::ghostty(100, 30, 1000, 570, Some(sized(10, 19)))]
        #[case::konsole_with_its_margins(100, 30, 1004, 570, Some(sized(10, 19)))]
        #[case::no_pixels(100, 30, 0, 0, None)]
        fn divides_the_window_by_its_cells(
            #[case] cols: u16,
            #[case] rows: u16,
            #[case] width: u16,
            #[case] height: u16,
            #[case] expected: Option<CellSize>,
        ) {
            assert_eq!(window_cell(cols, rows, width, height), expected);
        }

        #[rstest]
        #[case::both_agree(Some(sized(10, 19)), Some(sized(10, 19)), Some(sized(10, 19)))]
        #[case::the_report_wins(Some(sized(9, 18)), Some(sized(10, 19)), Some(sized(9, 18)))]
        #[case::only_the_window(None, Some(sized(10, 19)), Some(sized(10, 19)))]
        #[case::an_implausible_report(Some(sized(1, 1)), Some(sized(10, 19)), Some(sized(10, 19)))]
        #[case::nothing_plausible(Some(sized(0, 0)), Some(sized(1, 2)), None)]
        fn prefers_the_cell_size_the_terminal_reports(
            #[case] reported: Option<CellSize>,
            #[case] window: Option<CellSize>,
            #[case] expected: Option<CellSize>,
        ) {
            let facts = Facts { replies: Replies { cell: reported, ..Replies::default() }, window, ..Facts::default() };

            assert_eq!(facts.cell(), expected);
        }

        #[test]
        fn keeps_the_reported_cell_size_when_only_the_window_grows() {
            let mut facts = Facts {
                replies: Replies { cell: Some(sized(9, 18)), ..Replies::default() },
                window: Some(sized(10, 19)),
                ..Facts::default()
            };
            facts.resized(Some(sized(10, 19)));

            assert_eq!(facts.cell(), Some(sized(9, 18)));
        }

        #[test]
        fn follows_the_window_after_the_font_changes() {
            let mut facts = Facts {
                replies: Replies { cell: Some(sized(10, 19)), ..Replies::default() },
                window: Some(sized(10, 19)),
                ..Facts::default()
            };
            facts.resized(Some(sized(15, 29)));

            assert_eq!(facts.cell(), Some(sized(15, 29)));
        }

        #[test]
        fn keeps_the_last_cell_size_when_the_window_loses_its_pixels() {
            let mut facts = Facts { window: Some(sized(10, 19)), ..Facts::default() };
            facts.resized(None);

            assert_eq!(facts.cell(), Some(sized(10, 19)));
        }
    }

    mod messages {
        use super::*;
        use crate::graphics::{Missing, kitty, tmux};

        fn every_missing() -> Vec<Missing> {
            vec![
                Missing::Off,
                Missing::Unknown { name: None },
                Missing::Unknown { name: Some("Bobcat 1.0".into()) },
                Missing::Cannot { name: "Alacritty".into() },
                Missing::VsCode,
                Missing::Tabby,
                Missing::Xterm,
                Missing::Foot,
                Missing::OldKonsole { version: "21.12".into() },
                Missing::OldKitty { version: "0.27.0".into() },
                Missing::GhosttyStorage,
                Missing::WezTermPixels,
                Missing::NoCellSize,
                Missing::TmuxPassthrough,
                Missing::TmuxSilent,
                Missing::TmuxOuter { name: None },
                Missing::TmuxOuter { name: Some("xterm 407".into()) },
                Missing::TmuxNoSixel { version: None },
                Missing::TmuxNoSixel { version: Some("3.3a".into()) },
                Missing::TmuxNested,
                Missing::Zellij,
                Missing::OldZellij { version: None },
                Missing::OldZellij { version: Some("0.44.1".into()) },
                Missing::Screen,
                Missing::Multiplexer { term: "screen-256color".into() },
            ]
        }

        const NAMES: [&str; 11] = [
            "Alacritty",
            "Bobcat",
            "CORNERCASE_IMAGES=off",
            "Ghostty",
            "GNU",
            "Konsole",
            "Tabby",
            "VS",
            "WezTerm",
            "Zellij",
            "Ghostty,",
        ];

        #[test]
        fn says_what_then_what_to_do() {
            for missing in every_missing() {
                let lines = missing.lines();
                assert!(lines.len() >= 2, "{missing:?} needs a line saying what to do");
                for line in &lines {
                    let first = line.split(' ').next().unwrap_or_default();
                    let lower = line.chars().next().is_some_and(|c| !c.is_uppercase());
                    assert!(lower || NAMES.contains(&first), "{missing:?}: {line:?} starts with a capital");
                    assert!(!line.ends_with('.'), "{missing:?}: {line:?} ends with a full stop");
                }
            }
        }

        #[test]
        fn has_an_id_of_its_own_for_the_log() {
            let mut ids: Vec<&str> = every_missing().iter().map(Missing::id).collect();
            ids.dedup();

            assert_eq!(ids.len(), 21);
        }

        #[rstest]
        #[case::passthrough(Missing::TmuxPassthrough, "set -g allow-passthrough on")]
        #[case::unknown(Missing::Unknown { name: None }, "CORNERCASE_IMAGES=kitty, iterm or sixel")]
        #[case::a_terminal_by_name(Missing::Cannot { name: "Alacritty".into() }, "Alacritty cannot show images")]
        #[case::old_kitty(Missing::OldKitty { version: "0.27.0".into() }, "kitty 0.27.0 is too old")]
        #[case::xterm(Missing::Xterm, "xterm -ti vt340")]
        #[case::ghostty(Missing::GhosttyStorage, "image-storage-limit")]
        #[case::outer_terminal(Missing::TmuxOuter { name: Some("xterm 407".into()) }, "cornercase cannot show images in xterm 407 through tmux")]
        #[case::tmux_3_3_has_no_all(Missing::TmuxPassthrough, "all instead of on (tmux 3.4 or later)")]
        #[case::tmux_config_folder(Missing::TmuxPassthrough, "~/.config/tmux/tmux.conf")]
        #[case::an_inactive_pane(Missing::TmuxSilent, "not the active one")]
        #[case::a_hidden_window(Missing::TmuxSilent, "covers a hidden window")]
        #[case::ghostty_reload(Missing::GhosttyStorage, "reload it (ctrl+shift+, or cmd+shift+, on macOS)")]
        #[case::tmux_without_sixel(Missing::TmuxNoSixel { version: Some("3.3a".into()) }, "this tmux (3.3a) cannot draw sixel images")]
        #[case::old_zellij(Missing::OldZellij { version: Some("0.44.1".into()) }, "update Zellij to 0.45 or later")]
        #[case::unnamed_multiplexer(Missing::Multiplexer { term: "screen-256color".into() }, "this multiplexer (TERM=screen-256color) cannot pass images through")]
        fn names_the_fix(#[case] missing: Missing, #[case] fix: &str) {
            assert!(missing.lines().join("\n").contains(fix), "{:?}", missing.lines());
        }

        #[test]
        fn forgets_both_kitty_ids_of_this_client() {
            let support = Support { protocol: Some(Protocol::Kitty), id_hi: 42, ..Support::default() };

            assert_eq!(
                support.forget(),
                [kitty::delete(kitty::id(42, 0xf0)), kitty::delete(kitty::id(42, 0xf1))].concat()
            );
        }

        #[test]
        fn forgets_them_through_tmux() {
            let support =
                Support { protocol: Some(Protocol::Kitty), tmux: Tmux::Wrap, id_hi: 42, ..Support::default() };
            let ids = [kitty::id(42, 0xf0), kitty::id(42, 0xf1)];

            assert_eq!(support.forget(), ids.map(|id| tmux::wrap(&kitty::delete(id))).concat());
        }

        #[rstest]
        #[case::sixel(Some(Protocol::Sixel { registers: 256, max: None }))]
        #[case::iterm(Some(Protocol::Iterm))]
        #[case::none(None)]
        fn sends_nothing_to_forget_without_kitty(#[case] protocol: Option<Protocol>) {
            assert_eq!(Support { protocol, id_hi: 42, ..Support::default() }.forget(), Vec::<u8>::new());
        }
    }
}
