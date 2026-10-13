use super::{CellSize, Missing, Protocol, Support, Tmux};

pub(crate) const OVERRIDE_ENV: &str = "CORNERCASE_IMAGES";
pub(crate) const KITTY_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
pub(crate) const KITTY_QUERY_ID: &str = "31";
pub(crate) const TMUX_PROBE: &str = "\x1bPtmux;\x1b\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\x1b\\\x1b\\\
     \x1bPtmux;\x1b\x1b[>q\x1b\\\x1bPtmux;\x1b\x1b[c\x1b\\";
pub(crate) const TMUX_FORMAT: &str = "#{pane_tty}|#{allow-passthrough}|#{client_termtype}|#{client_termfeatures}";
pub(crate) const PANE_TITLE: &str = "\x1b]2;cornercase\x1b\\";
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
pub(crate) struct Replies {
    pub(crate) terminal: Option<String>,
    pub(crate) attributes: Option<Vec<u32>>,
    pub(crate) kitty: Option<String>,
    pub(crate) cell: Option<CellSize>,
    pub(crate) registers: Option<u32>,
    pub(crate) geometry: Option<(u32, u32)>,
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
pub(crate) enum Outer {
    Local { passthrough: bool, terminal: Option<String>, sixel: bool },
    Probed(Replies),
    Stale,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) replies: Replies,
    pub(crate) asked_kitty: bool,
    pub(crate) window: Option<CellSize>,
    pub(crate) tmux: Option<Outer>,
}

impl Facts {
    pub(crate) fn cell(&self) -> Option<CellSize> {
        self.replies.cell.filter(|cell| plausible(*cell)).or(self.window.filter(|cell| plausible(*cell)))
    }

    pub(crate) fn resized(&mut self, window: Option<CellSize>) {
        if window.is_some() && window != self.window {
            self.replies.cell = None;
            self.window = window;
        }
    }

    pub(crate) fn checks_tmux_again(&self) -> bool {
        matches!(self.tmux, Some(Outer::Local { .. }))
    }

    pub(crate) fn renamed_tmux_pane(&self) -> bool {
        self.asked_kitty && self.replies.terminal.as_deref().is_some_and(is_tmux)
    }

    pub(crate) fn summary(&self) -> String {
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

pub(crate) fn window_cell(cols: u16, rows: u16, width: u16, height: u16) -> Option<CellSize> {
    (cols > 0 && rows > 0 && width > 0 && height > 0).then(|| CellSize { width: width / cols, height: height / rows })
}

fn plausible(cell: CellSize) -> bool {
    let (w, h) = (u32::from(cell.width), u32::from(cell.height));
    (2..=256).contains(&w) && (4..=512).contains(&h) && w <= h * 2 && h <= w * 8
}

pub(crate) fn overridden(var: impl Fn(&str) -> Option<String>) -> bool {
    var(OVERRIDE_ENV).is_some_and(|value| forced(&value, &Replies::default(), false).is_some())
}

pub(crate) fn ignored_override(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    var(OVERRIDE_ENV).filter(|value| forced(value, &Replies::default(), false).is_none())
}

pub(crate) fn in_multiplexer(var: impl Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let term = var("TERM").unwrap_or_default();
    set("TMUX") || set("STY") || set("ZELLIJ") || term.starts_with("tmux") || term.starts_with("screen")
}

pub(crate) fn likely_kitty(var: impl Fn(&str) -> Option<String>) -> bool {
    let is = |name: &str, values: &[&str]| var(name).is_some_and(|v| values.iter().any(|w| v.eq_ignore_ascii_case(w)));
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    is("TERM", &["xterm-kitty", "xterm-ghostty", "xterm-rio", "rio"])
        || is("TERM_PROGRAM", &["ghostty", "rio"])
        || set("KITTY_WINDOW_ID")
        || set("GHOSTTY_RESOURCES_DIR")
}

pub(crate) fn in_tmux(terminal: Option<&str>, var: impl Fn(&str) -> Option<String>) -> bool {
    multiplexer(terminal, &var, true) == Some(Mux::Tmux)
}

pub(crate) fn local_tmux(output: &str, own_tty: Option<&str>) -> Option<Outer> {
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

pub(crate) fn decide(facts: &Facts, var: impl Fn(&str) -> Option<String>) -> Support {
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

pub(crate) fn version(text: &str) -> Option<Version> {
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
mod tests;
