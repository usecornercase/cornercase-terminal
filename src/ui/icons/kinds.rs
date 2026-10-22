use std::path::Path;

use ratatui::style::{Color, Style};

pub(crate) const SLOT: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Icon {
    pub(crate) glyph: &'static str,
    pub(crate) colour: Option<Color>,
}

impl Icon {
    pub(crate) fn style(self, soft: Color) -> Style {
        Style::default().fg(self.colour.unwrap_or(soft))
    }
}

const fn icon(glyph: &'static str, colour: Option<Color>) -> Icon {
    Icon { glyph, colour }
}

const FOLDER: Icon = icon("\u{e5ff}", Some(Color::Blue));
const FOLDER_OPEN: Icon = icon("\u{e5fe}", Some(Color::Blue));
const GENERIC: Icon = icon("\u{e64e}", None);

pub(super) const NAMES: &[(&[&str], Icon)] = &[
    (&["Cargo.toml"], icon("\u{e68b}", Some(Color::Red))),
    (&["Cargo.lock", "package-lock.json", "pnpm-lock.yaml", "go.sum"], icon("\u{e672}", None)),
    (&["package.json"], icon("\u{e616}", Some(Color::Red))),
    (&["tsconfig.json", "jsconfig.json"], icon("\u{e69d}", Some(Color::Blue))),
    (
        &[
            "Dockerfile",
            "Containerfile",
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
            ".dockerignore",
        ],
        icon("\u{e650}", Some(Color::Blue)),
    ),
    (&["Makefile", "GNUmakefile"], icon("\u{e673}", Some(Color::Yellow))),
    (&[".gitignore", ".gitattributes", ".gitmodules", ".gitkeep"], icon("\u{e65d}", Some(Color::Red))),
    (&[".env", ".envrc", ".env.local", ".env.example"], icon("\u{e615}", Some(Color::Yellow))),
    (&["README", "README.md"], icon("\u{e66a}", Some(Color::Cyan))),
    (&["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "COPYING"], icon("\u{e60a}", Some(Color::Yellow))),
    (&[".editorconfig"], icon("\u{e652}", None)),
    (&["Gemfile", "Rakefile"], icon("\u{e605}", Some(Color::Red))),
    (&["go.mod"], icon("\u{e627}", Some(Color::Cyan))),
    (&[".bashrc", ".zshrc", ".profile", ".bash_profile"], icon("\u{e691}", Some(Color::Green))),
];

pub(super) const EXTENSIONS: &[(&[&str], Icon)] = &[
    (&["rs"], icon("\u{e68b}", Some(Color::Red))),
    (&["ts", "mts", "cts"], icon("\u{e628}", Some(Color::Blue))),
    (&["tsx"], icon("\u{e625}", Some(Color::Blue))),
    (&["js", "mjs", "cjs"], icon("\u{e60c}", Some(Color::Yellow))),
    (&["jsx"], icon("\u{e625}", Some(Color::Cyan))),
    (&["py", "pyi"], icon("\u{e606}", Some(Color::Yellow))),
    (&["go"], icon("\u{e627}", Some(Color::Cyan))),
    (&["rb"], icon("\u{e605}", Some(Color::Red))),
    (&["java"], icon("\u{e66d}", Some(Color::Red))),
    (&["kt", "kts"], icon("\u{e634}", Some(Color::Magenta))),
    (&["swift"], icon("\u{e699}", Some(Color::Red))),
    (&["c"], icon("\u{e649}", Some(Color::Blue))),
    (&["h"], icon("\u{e649}", Some(Color::Magenta))),
    (&["cpp", "cc", "cxx"], icon("\u{e646}", Some(Color::Blue))),
    (&["hpp", "hh", "hxx"], icon("\u{e646}", Some(Color::Magenta))),
    (&["cs"], icon("\u{e648}", Some(Color::Green))),
    (&["php"], icon("\u{e608}", Some(Color::Magenta))),
    (&["lua"], icon("\u{e620}", Some(Color::Blue))),
    (&["sh", "bash", "zsh", "fish"], icon("\u{e691}", Some(Color::Green))),
    (&["toml"], icon("\u{e6b2}", Some(Color::Red))),
    (&["yaml", "yml"], icon("\u{e6a8}", Some(Color::Magenta))),
    (&["json", "jsonc", "json5"], icon("\u{e60b}", Some(Color::Yellow))),
    (&["md", "markdown", "mdx"], icon("\u{e609}", Some(Color::Blue))),
    (&["html", "htm"], icon("\u{e60e}", Some(Color::Red))),
    (&["css"], icon("\u{e614}", Some(Color::Blue))),
    (&["scss", "sass"], icon("\u{e603}", Some(Color::Magenta))),
    (&["sql", "db", "sqlite"], icon("\u{e64d}", Some(Color::Cyan))),
    (&["vue"], icon("\u{e6a0}", Some(Color::Green))),
    (&["svelte"], icon("\u{e697}", Some(Color::Red))),
    (&["graphql", "gql"], icon("\u{e662}", Some(Color::Magenta))),
    (&["ex", "exs"], icon("\u{e62d}", Some(Color::Magenta))),
    (&["hs"], icon("\u{e61f}", Some(Color::Magenta))),
    (&["scala"], icon("\u{e68e}", Some(Color::Red))),
    (&["dart"], icon("\u{e64c}", Some(Color::Cyan))),
    (&["zig"], icon("\u{e6a9}", Some(Color::Yellow))),
    (&["tf", "tfvars"], icon("\u{e69a}", Some(Color::Magenta))),
    (&["vim"], icon("\u{e62b}", Some(Color::Green))),
    (&["xml"], icon("\u{e619}", Some(Color::Yellow))),
    (&["svg"], icon("\u{e698}", Some(Color::Yellow))),
    (&["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"], icon("\u{e60d}", Some(Color::Magenta))),
    (&["pdf"], icon("\u{e67d}", Some(Color::Red))),
    (&["zip", "tar", "gz", "tgz", "xz", "bz2", "7z", "zst", "rar"], icon("\u{e6aa}", Some(Color::Yellow))),
    (&["lock", "lockb"], icon("\u{e672}", None)),
    (&["csv", "tsv"], icon("\u{e64a}", Some(Color::Green))),
    (&["ini", "conf", "cfg"], icon("\u{e615}", None)),
    (&["env"], icon("\u{e615}", Some(Color::Yellow))),
    (&["ttf", "otf", "woff", "woff2"], icon("\u{e659}", None)),
    (&["mp3", "wav", "flac", "ogg"], icon("\u{e638}", Some(Color::Cyan))),
    (&["mp4", "mov", "webm", "mkv"], icon("\u{e69f}", Some(Color::Magenta))),
];

fn find(table: &[(&[&str], Icon)], key: &str) -> Option<Icon> {
    table.iter().find(|(keys, _)| keys.iter().any(|k| k.eq_ignore_ascii_case(key))).map(|(_, icon)| *icon)
}

pub(crate) fn folder(open: bool) -> Icon {
    if open { FOLDER_OPEN } else { FOLDER }
}

pub(crate) fn file(name: &str) -> Icon {
    find(NAMES, name)
        .or_else(|| Path::new(name).extension().and_then(|ext| ext.to_str()).and_then(|ext| find(EXTENSIONS, ext)))
        .unwrap_or(GENERIC)
}

pub(crate) fn file_at(path: &str) -> Icon {
    file(path.rsplit('/').next().unwrap_or(path))
}

#[cfg(test)]
mod tests;
