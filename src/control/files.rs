use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use super::Fields;
use super::changes::{self, FileStatus};
use crate::changes::diff::Status;
use crate::files::disk::{self, Body, Raw};
use crate::files::search;
use crate::graphics::decode;
use crate::highlight::syntax;
use crate::time::Stamp;

const IMAGE: &str = "image";
const DAMAGED: &str = "the image data is damaged";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FilesList {
    pub(crate) workspace: Option<u64>,
    pub(crate) folders: Vec<String>,
    pub(crate) follow: bool,
}

impl FilesList {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("folders", self.folders.len()).add("follow", self.follow);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FilesRead {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) follow: bool,
}

impl FilesRead {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path).add("follow", self.follow);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchMode {
    #[default]
    Text,
    Names,
}

impl SearchMode {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Names => "names",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FilesSearch {
    pub(crate) workspace: Option<u64>,
    pub(crate) query: String,
    pub(crate) mode: SearchMode,
}

impl FilesSearch {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("mode", self.mode.name()).add("bytes", self.query.len());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Line<T> {
    pub(crate) boot: String,
    pub(crate) stream: Option<u64>,
    pub(crate) revision: u64,
    #[serde(flatten)]
    pub(crate) content: T,
}

impl<T: Serialize> Line<T> {
    pub(crate) fn streamed(&self) -> Option<String> {
        super::streamed_line(self)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EntryKind {
    #[default]
    File,
    Folder,
    Other,
}

impl From<disk::Kind> for EntryKind {
    fn from(kind: disk::Kind) -> Self {
        match kind {
            disk::Kind::File => Self::File,
            disk::Kind::Folder => Self::Folder,
            disk::Kind::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct EntryInfo {
    pub(crate) name: String,
    pub(crate) kind: EntryKind,
    pub(crate) link: bool,
    pub(crate) hidden: bool,
    pub(crate) bytes: Option<u64>,
    pub(crate) modified: Option<String>,
    pub(crate) status: Option<FileStatus>,
}

impl EntryInfo {
    pub(crate) fn of(listed: &disk::Listed, status: Option<Status>) -> Self {
        Self {
            name: listed.name.clone(),
            kind: listed.kind.into(),
            link: listed.link,
            hidden: listed.name.starts_with('.'),
            bytes: listed.bytes,
            modified: listed.modified.map(|at| Stamp(at).to_string()),
            status: status.map(Into::into),
        }
    }
}

impl From<&EntryInfo> for disk::Entry {
    fn from(entry: &EntryInfo) -> Self {
        Self { name: entry.name.clone(), dir: entry.kind == EntryKind::Folder }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FolderInfo {
    pub(crate) path: String,
    pub(crate) dir: PathBuf,
    pub(crate) entries: Vec<EntryInfo>,
    pub(crate) more: bool,
    pub(crate) error: Option<String>,
}

impl FolderInfo {
    pub(crate) fn failed(path: &str, dir: &Path, error: &str) -> Self {
        Self { path: path.into(), dir: dir.to_path_buf(), error: Some(error.into()), ..Self::default() }
    }

    pub(crate) fn listed(&self) -> (String, Option<Vec<disk::Entry>>) {
        let entries = self.error.is_none().then(|| self.entries.iter().map(Into::into).collect());
        (self.path.clone(), entries)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Listing {
    pub(crate) workspace: Option<u64>,
    pub(crate) folders: Vec<FolderInfo>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReadKind {
    #[default]
    Text,
    Image,
    Binary,
    TooLarge,
    Folder,
    Missing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ImageInfo {
    pub(crate) format: String,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) data: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Read {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) file: PathBuf,
    pub(crate) kind: ReadKind,
    pub(crate) bytes: Option<u64>,
    pub(crate) modified: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) image: Option<ImageInfo>,
}

impl Read {
    pub(crate) fn of(path: &str, file: &Path, loaded: disk::Loaded) -> Self {
        let disk::Loaded { stamp, raw } = loaded;
        let mut read = Self {
            path: path.into(),
            file: file.to_path_buf(),
            bytes: stamp.len(),
            modified: stamp.modified().map(|at| Stamp(at).to_string()),
            ..Self::default()
        };
        match raw {
            Raw::Text(text) => {
                let first = syntax::lines(&text).next().map_or("", |(_, line)| line);
                read.language = syntax::language(file, first).map(str::to_string);
                read.text = Some(text);
            }
            Raw::Image { format, bytes } => {
                let (width, height) = decode::dimensions(&bytes).unzip();
                let data = STANDARD.encode(&bytes);
                read.kind = ReadKind::Image;
                read.image = Some(ImageInfo { format: format.into(), width, height, data });
            }
            Raw::Binary => read.kind = ReadKind::Binary,
            Raw::TooLarge(size) => {
                read.kind = ReadKind::TooLarge;
                read.bytes = Some(size);
            }
            Raw::Folder => {
                read.kind = ReadKind::Folder;
                read.bytes = None;
            }
            Raw::Missing => {
                read.kind = ReadKind::Missing;
                read.bytes = None;
            }
        }
        read
    }

    pub(crate) fn content(&self) -> disk::Content {
        let stamp = disk::Stamp::of(self.bytes, self.modified.as_deref().and_then(Stamp::parse));
        let (language, body) = match self.kind {
            ReadKind::Text => {
                (self.language.as_deref().and_then(syntax::named), disk::text(self.text.clone().unwrap_or_default()))
            }
            ReadKind::Image => (None, self.picture()),
            ReadKind::Binary | ReadKind::Folder => (None, Body::Binary),
            ReadKind::TooLarge => (None, Body::TooLarge(self.bytes.unwrap_or_default())),
            ReadKind::Missing => (None, Body::Missing),
        };
        disk::Content { stamp, language, body }
    }

    fn picture(&self) -> Body {
        match self.image.as_ref().map(|image| STANDARD.decode(&image.data)) {
            Some(Ok(bytes)) => disk::image(decode::sniff(&bytes).unwrap_or(IMAGE), &bytes),
            _ => Body::Unreadable { format: IMAGE, bytes: self.bytes.unwrap_or_default(), reason: DAMAGED.into() },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NameInfo {
    pub(crate) path: String,
    pub(crate) indices: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FoundLine {
    pub(crate) number: u32,
    pub(crate) text: String,
    pub(crate) ranges: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FoundFile {
    pub(crate) path: String,
    pub(crate) lines: Vec<FoundLine>,
}

impl NameInfo {
    fn name(&self) -> search::Name {
        let chars = u32::try_from(self.path.chars().count()).unwrap_or(u32::MAX);
        search::Name { path: self.path.clone(), indices: self.indices.iter().copied().filter(|&i| i < chars).collect() }
    }
}

impl FoundFile {
    fn found(&self) -> search::Found {
        let line = |line: &FoundLine| search::Line {
            number: line.number,
            text: line.text.clone(),
            ranges: changes::within(&line.ranges, &line.text),
        };
        search::Found { path: self.path.clone(), lines: self.lines.iter().map(line).collect() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Found {
    pub(crate) workspace: u64,
    pub(crate) query: String,
    pub(crate) mode: SearchMode,
    pub(crate) names: Vec<NameInfo>,
    pub(crate) files: Vec<FoundFile>,
    pub(crate) matches: usize,
    pub(crate) capped: bool,
}

impl Found {
    pub(crate) fn names(workspace: u64, query: &str, found: &[search::Name]) -> Self {
        Self {
            workspace,
            query: query.into(),
            mode: SearchMode::Names,
            names: found.iter().map(|n| NameInfo { path: n.path.clone(), indices: n.indices.clone() }).collect(),
            matches: found.len(),
            capped: found.len() >= search::MAX_NAMES,
            ..Self::default()
        }
    }

    pub(crate) fn text(workspace: u64, query: &str, found: &search::Text) -> Self {
        let line = |line: &search::Line| FoundLine {
            number: line.number,
            text: line.text.clone(),
            ranges: line.ranges.iter().map(|r| [r.start, r.end]).collect(),
        };
        let file =
            |file: &search::Found| FoundFile { path: file.path.clone(), lines: file.lines.iter().map(line).collect() };
        Self {
            workspace,
            query: query.into(),
            mode: SearchMode::Text,
            files: found.files.iter().map(file).collect(),
            matches: found.matches,
            capped: found.capped,
            ..Self::default()
        }
    }

    pub(crate) fn found_names(&self) -> Vec<search::Name> {
        self.names.iter().map(NameInfo::name).collect()
    }

    pub(crate) fn found_text(&self) -> search::Text {
        search::Text {
            files: self.files.iter().map(FoundFile::found).collect(),
            matches: self.matches,
            capped: self.capped,
        }
    }
}

#[cfg(test)]
mod tests;
