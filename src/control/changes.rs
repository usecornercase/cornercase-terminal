use std::ops::Range;

use ratatui::style::Style as Look;
use serde::{Deserialize, Serialize};

use super::Fields;
use super::surface::{Style, Table};
use crate::changes::diff::{self, Diff, File, Staged};
use crate::changes::merge::{Conflict, Merge, With};
use crate::changes::{GapLine, Mode, Scope};
use crate::term::emulator::Decor;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Changes {
    pub(crate) workspace: Option<u64>,
    pub(crate) scope: Scope,
    pub(crate) mode: Mode,
}

impl Changes {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("scope", self.scope.name()).add("mode", self.mode.name());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesMode {
    pub(crate) stream: Option<u64>,
    pub(crate) mode: Mode,
}

impl ChangesMode {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("stream", self.stream).add("mode", self.mode.name());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesFile {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) mode: Mode,
}

impl ChangesFile {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path).add("mode", self.mode.name());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesGap {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) hunk: usize,
    pub(crate) mode: Mode,
}

impl ChangesGap {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path).add("hunk", self.hunk);
        fields.add("mode", self.mode.name());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesBase {
    pub(crate) workspace: Option<u64>,
    pub(crate) base: Option<String>,
    pub(crate) clear: bool,
}

impl ChangesBase {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("base", self.base.is_some()).add("clear", self.clear);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesViewed {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) digest: Option<String>,
    pub(crate) clear: bool,
}

impl ChangesViewed {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path).add("clear", self.clear);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesDiscard {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) digest: Option<String>,
}

impl ChangesDiscard {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesStage {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: String,
    pub(crate) digest: Option<String>,
    #[serde(default = "to_stage")]
    pub(crate) staged: bool,
}

fn to_stage() -> bool {
    true
}

impl ChangesStage {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("path", &self.path).add("staged", self.staged);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ChangesMergeCheck {
    pub(crate) workspace: Option<u64>,
    pub(crate) with: Option<u64>,
    pub(crate) base: Option<String>,
}

impl ChangesMergeCheck {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).maybe("with", self.with).add("base", self.base.is_some());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MergeChecked {
    pub(crate) workspace: u64,
    pub(crate) checks: Vec<MergeInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MergeInfo {
    pub(crate) with: Option<u64>,
    pub(crate) base: Option<String>,
    pub(crate) clean: bool,
    pub(crate) conflicts: Vec<ConflictInfo>,
    pub(crate) left_out: usize,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConflictInfo {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) message: String,
}

impl From<&Merge> for MergeInfo {
    fn from(merge: &Merge) -> Self {
        let (with, base) = match &merge.with {
            With::Workspace(id) => (Some(*id), None),
            With::Base(base) => (None, base.clone()),
        };
        let found = Self { with, base, ..Self::default() };
        match &merge.result {
            Ok(merged) => Self {
                clean: merged.conflicts.is_empty(),
                conflicts: merged.conflicts.iter().map(ConflictInfo::from).collect(),
                left_out: merged.left_out,
                ..found
            },
            Err(why) => Self { error: Some(why.clone()), ..found },
        }
    }
}

impl From<&Conflict> for ConflictInfo {
    fn from(conflict: &Conflict) -> Self {
        Self { path: conflict.path.clone(), kind: conflict.kind.clone(), message: conflict.message.clone() }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    #[default]
    Loading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Staging {
    #[default]
    No,
    Partly,
    Yes,
}

impl From<Staged> for Staging {
    fn from(staged: Staged) -> Self {
        match staged {
            Staged::No => Self::No,
            Staged::Partly => Self::Partly,
            Staged::Yes => Self::Yes,
        }
    }
}

impl From<Staging> for Staged {
    fn from(staging: Staging) -> Self {
        match staging {
            Staging::No => Self::No,
            Staging::Partly => Self::Partly,
            Staging::Yes => Self::Yes,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FileStatus {
    #[default]
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl From<FileStatus> for diff::Status {
    fn from(status: FileStatus) -> Self {
        match status {
            FileStatus::Modified => Self::Modified,
            FileStatus::Added => Self::Added,
            FileStatus::Deleted => Self::Deleted,
            FileStatus::Renamed => Self::Renamed,
            FileStatus::Untracked => Self::Untracked,
        }
    }
}

impl From<diff::Status> for FileStatus {
    fn from(status: diff::Status) -> Self {
        match status {
            diff::Status::Modified => Self::Modified,
            diff::Status::Added => Self::Added,
            diff::Status::Deleted => Self::Deleted,
            diff::Status::Renamed => Self::Renamed,
            diff::Status::Untracked => Self::Untracked,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FoldClass {
    #[default]
    Open,
    Lockfile,
    Binary,
    Large,
}

impl From<diff::Fold> for FoldClass {
    fn from(fold: diff::Fold) -> Self {
        match fold {
            diff::Fold::Open => Self::Open,
            diff::Fold::Lockfile => Self::Lockfile,
            diff::Fold::Binary => Self::Binary,
            diff::Fold::Large => Self::Large,
        }
    }
}

impl From<FoldClass> for diff::Fold {
    fn from(fold: FoldClass) -> Self {
        match fold {
            FoldClass::Open => Self::Open,
            FoldClass::Lockfile => Self::Lockfile,
            FoldClass::Binary => Self::Binary,
            FoldClass::Large => Self::Large,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LineKind {
    #[default]
    Context,
    Removed,
    Added,
}

impl From<diff::Kind> for LineKind {
    fn from(kind: diff::Kind) -> Self {
        match kind {
            diff::Kind::Context => Self::Context,
            diff::Kind::Removed => Self::Removed,
            diff::Kind::Added => Self::Added,
        }
    }
}

impl From<LineKind> for diff::Kind {
    fn from(kind: LineKind) -> Self {
        match kind {
            LineKind::Context => Self::Context,
            LineKind::Removed => Self::Removed,
            LineKind::Added => Self::Added,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FileInfo {
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    pub(crate) status: FileStatus,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) fold: FoldClass,
    pub(crate) viewed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) staged: Option<Staging>,
    pub(crate) digest: String,
}

impl FileInfo {
    pub(crate) fn of(file: &File, viewed: bool) -> Self {
        Self {
            path: file.path.clone(),
            old_path: file.old_path.clone(),
            status: file.status.into(),
            added: file.added,
            removed: file.removed,
            fold: file.fold.into(),
            viewed,
            staged: file.staged.map(Into::into),
            digest: digest(file.viewed_digest),
        }
    }

    pub(crate) fn file(&self) -> File {
        let digest = parse_digest(&self.digest).unwrap_or_default();
        File {
            path: self.path.clone(),
            old_path: self.old_path.clone(),
            status: self.status.into(),
            added: self.added,
            removed: self.removed,
            fold: self.fold.into(),
            hunks: Vec::new(),
            digest,
            viewed_digest: digest,
            staged: self.staged.map(Into::into),
        }
    }
}

pub(crate) fn digest(value: u64) -> String {
    format!("{value:016x}")
}

pub(crate) fn parse_digest(text: &str) -> Option<u64> {
    (text.len() == 16 && text.bytes().all(|b| b.is_ascii_hexdigit())).then(|| u64::from_str_radix(text, 16).ok())?
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorkspaceChanges {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) base: Option<String>,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) files: Vec<FileInfo>,
}

impl WorkspaceChanges {
    pub(crate) fn of(
        project: u64,
        workspace: u64,
        base: Option<String>,
        diff: &Diff,
        viewed: impl Fn(&File) -> bool,
    ) -> Self {
        Self {
            project,
            workspace,
            base,
            added: diff.added(),
            removed: diff.removed(),
            files: diff.files.iter().map(|file| FileInfo::of(file, viewed(file))).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Content {
    pub(crate) scope: Scope,
    pub(crate) mode: Mode,
    pub(crate) state: State,
    pub(crate) error: Option<String>,
    pub(crate) workspaces: Vec<WorkspaceChanges>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Summary {
    pub(crate) boot: String,
    pub(crate) stream: u64,
    pub(crate) revision: u64,
    #[serde(flatten)]
    pub(crate) content: Content,
}

impl Summary {
    pub(crate) fn streamed(&self) -> Option<String> {
        super::streamed_line(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Segment {
    pub(crate) style: usize,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LineInfo {
    pub(crate) kind: LineKind,
    pub(crate) old: Option<u32>,
    pub(crate) new: Option<u32>,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) emphasis: Vec<[usize; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) syntax: Option<Vec<Segment>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct HunkInfo {
    pub(crate) old_start: u32,
    pub(crate) new_start: u32,
    pub(crate) context: String,
    pub(crate) lines: Vec<LineInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Patch {
    pub(crate) workspace: u64,
    pub(crate) mode: Mode,
    #[serde(flatten)]
    pub(crate) file: FileInfo,
    pub(crate) styles: Vec<Style>,
    pub(crate) hunks: Vec<HunkInfo>,
}

fn segments(syntax: &[(String, Look)], styles: &mut Table<Style>) -> Vec<Segment> {
    syntax
        .iter()
        .map(|(text, look)| Segment { style: styles.of(Style::of(*look, Decor::default())), text: text.clone() })
        .collect()
}

fn table() -> Table<Style> {
    Table::seeded(Some(Style::default()))
}

impl Patch {
    pub(crate) fn of(workspace: u64, mode: Mode, file: &File, viewed: bool) -> Self {
        let mut styles = table();
        let hunks = file
            .hunks
            .iter()
            .map(|hunk| HunkInfo {
                old_start: hunk.old_start,
                new_start: hunk.new_start,
                context: hunk.context.clone(),
                lines: hunk
                    .lines
                    .iter()
                    .map(|line| LineInfo {
                        kind: line.kind.into(),
                        old: line.old,
                        new: line.new,
                        text: line.text.clone(),
                        emphasis: line.emphasis.iter().map(|r| [r.start, r.end]).collect(),
                        syntax: line.syntax.as_deref().map(|s| segments(s, &mut styles)),
                    })
                    .collect(),
            })
            .collect();
        Self { workspace, mode, file: FileInfo::of(file, viewed), styles: styles.into_entries(), hunks }
    }

    pub(crate) fn file(&self) -> File {
        let looks = looks(&self.styles);
        File { hunks: self.hunks.iter().map(|hunk| hunk.hunk(&looks)).collect(), ..self.file.file() }
    }
}

impl HunkInfo {
    fn hunk(&self, looks: &[Look]) -> diff::Hunk {
        diff::Hunk {
            old_start: self.old_start,
            new_start: self.new_start,
            context: self.context.clone(),
            lines: self.lines.iter().map(|line| line.line(looks)).collect(),
        }
    }
}

impl LineInfo {
    fn line(&self, looks: &[Look]) -> diff::Line {
        diff::Line {
            kind: self.kind.into(),
            old: self.old,
            new: self.new,
            text: self.text.clone(),
            raw: None,
            syntax: self.syntax.as_deref().map(|segments| looked(segments, looks)),
            emphasis: within(&self.emphasis, &self.text),
        }
    }
}

fn looks(styles: &[Style]) -> Vec<Look> {
    styles.iter().map(|style| style.look().0).collect()
}

fn looked(segments: &[Segment], looks: &[Look]) -> diff::Segments {
    segments
        .iter()
        .map(|segment| (segment.text.clone(), looks.get(segment.style).copied().unwrap_or_default()))
        .collect()
}

pub(super) fn within(ranges: &[[usize; 2]], text: &str) -> Vec<Range<usize>> {
    let chars = text.chars().count();
    ranges.iter().map(|&[start, end]| start..end.min(chars)).filter(|range| !range.is_empty()).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GapLineInfo {
    pub(crate) old: u32,
    pub(crate) new: u32,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) syntax: Option<Vec<Segment>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Gap {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) hunk: usize,
    pub(crate) styles: Vec<Style>,
    pub(crate) lines: Vec<GapLineInfo>,
}

impl Gap {
    pub(crate) fn of(workspace: u64, path: &str, hunk: usize, lines: &[GapLine]) -> Self {
        let mut styles = table();
        let lines = lines
            .iter()
            .map(|line| GapLineInfo {
                old: line.old,
                new: line.new,
                text: line.text.clone(),
                syntax: line.syntax.as_deref().map(|s| segments(s, &mut styles)),
            })
            .collect();
        Self { workspace, path: path.to_string(), hunk, styles: styles.into_entries(), lines }
    }

    pub(crate) fn lines(&self) -> Vec<GapLine> {
        let looks = looks(&self.styles);
        self.lines
            .iter()
            .map(|line| GapLine {
                old: line.old,
                new: line.new,
                text: line.text.clone(),
                raw: None,
                syntax: line.syntax.as_deref().map(|segments| looked(segments, &looks)),
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "`base` is the key `changes-base` answers the workspace's base by")]
pub(crate) struct Base {
    pub(crate) workspace: u64,
    pub(crate) base: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) branches: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_field_names, reason = "`viewed` is the key `changes-viewed` answers with")]
pub(crate) struct Viewed {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) viewed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StagedFile {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    pub(crate) status: FileStatus,
    pub(crate) staged: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Discarded {
    pub(crate) workspace: u64,
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    pub(crate) status: FileStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Following {
    pub(crate) stream: u64,
    pub(crate) scope: Scope,
    pub(crate) workspace: Option<u64>,
    pub(crate) mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChangesCount {
    pub(crate) files: u64,
    pub(crate) added: u64,
    pub(crate) removed: u64,
    pub(crate) mode: Mode,
}

#[cfg(test)]
mod tests;
