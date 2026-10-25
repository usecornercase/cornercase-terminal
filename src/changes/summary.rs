use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use super::Mode;
use super::diff::{self, Diff, File, Fold, Status};
use super::git::{self, Loaded, Request};
use super::porcelain::{Porcelain, path_prefix};
use crate::error::Result;

fn counted(raw: &[u8], statuses: &Statuses) -> Vec<File> {
    let mut records = raw.split(|b| *b == 0);
    let mut files = Vec::new();
    while let Some(record) = records.next().filter(|r| !r.is_empty()) {
        let text = String::from_utf8_lossy(record);
        let fields: Vec<&str> = text.splitn(3, '\t').collect();
        let [added, removed, path] = fields.as_slice() else { continue };
        let (path, old_path) = if path.is_empty() {
            let old = records.next().map(|r| String::from_utf8_lossy(r).into_owned());
            let Some(new) = records.next() else { continue };
            (String::from_utf8_lossy(new).into_owned(), old)
        } else {
            ((*path).to_string(), None)
        };
        let status =
            if old_path.is_some() { Status::Renamed } else { statuses.get(&path).copied().unwrap_or(Status::Modified) };
        let binary = *added == "-";
        let (added, removed) = (added.parse().unwrap_or(0), removed.parse().unwrap_or(0));
        let fold = Fold::of(&path, binary, added, removed);
        files.push(File {
            path,
            old_path,
            status,
            added,
            removed,
            fold,
            hunks: Vec::new(),
            digest: 0,
            viewed_digest: 0,
            staged: None,
        });
    }
    files
}

type Statuses = HashMap<String, Status>;
type Previous<'a> = HashMap<&'a str, &'a Arc<File>>;

pub(super) fn load(request: &Request, expanded: &[String]) -> Result<Loaded> {
    if request.mode != Mode::Uncommitted {
        return compared(request, expanded);
    }
    let dir = request.dir.as_path();
    let from = git::head(dir)?;
    let status = git::status(dir)?;
    let numstat = numstat(dir, &from)?;
    let porcelain = Porcelain::parse(&status, &path_prefix(dir));
    let untracked = git::read_untracked(dir, porcelain.untracked.clone());
    let previous: Previous = request.previous.iter().map(|f| (f.path.as_str(), f)).collect();
    let paths = patch_paths(expanded, &untracked, &porcelain.renames, &previous);
    let patch = if paths.is_empty() { Vec::new() } else { git::patch(dir, &from, &paths)? };
    let digest = digest_of(dir, &porcelain.statuses, (&from, &status, &numstat, &untracked, expanded, &patch));
    if request.last == Some(digest) {
        return Ok(Loaded { base: None, digest, diff: None });
    }
    let files = summarized(dir, &from, &numstat, &porcelain, &untracked);
    let mut patches: HashMap<String, File> =
        diff::parse(&String::from_utf8_lossy(&patch)).into_iter().map(|f| (f.path.clone(), f)).collect();
    let mut files: Vec<_> = files
        .into_iter()
        .map(|file| {
            let file = if expanded.contains(&file.path) { opened(file, &untracked, &mut patches) } else { file };
            git::reuse(file, &previous)
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded { base: None, digest, diff: Some(Diff { files }) })
}

fn numstat(dir: &Path, from: &str) -> Result<Vec<u8>> {
    git::run(
        dir,
        &["diff", "--numstat", "-z", "--find-renames", "--relative", "--no-ext-diff", "--no-textconv", from, "--", "."],
    )
}

fn patch_paths<'a>(
    expanded: &'a [String],
    untracked: &[git::Untracked],
    renames: &'a HashMap<String, String>,
    previous: &Previous<'a>,
) -> Vec<&'a str> {
    expanded
        .iter()
        .filter(|path| !untracked.iter().any(|u| u.path == **path))
        .flat_map(|path| {
            let old = renames
                .get(path)
                .map(String::as_str)
                .or_else(|| previous.get(path.as_str()).and_then(|file| file.old_path.as_deref()));
            std::iter::once(path.as_str()).chain(old)
        })
        .collect()
}

fn digest_of(dir: &Path, statuses: &Statuses, parts: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    let mut tracked: Vec<_> = statuses.keys().collect();
    tracked.sort();
    for path in tracked {
        (path, git::stamp(&dir.join(path))).hash(&mut hasher);
    }
    hasher.finish()
}

fn summarized(
    dir: &Path,
    from: &str,
    numstat: &[u8],
    porcelain: &Porcelain,
    untracked: &[git::Untracked],
) -> Vec<File> {
    let mut files = counted(numstat, &porcelain.statuses);
    files.extend(untracked.iter().map(|u| diff::untracked_file(&u.path, u.size, u.bytes.as_deref(), false)));
    for file in &mut files {
        file.viewed_digest = git::fingerprint(dir, from, file);
        file.digest = file.viewed_digest;
        file.staged = Some(porcelain.staged(file));
    }
    files
}

fn opened(file: File, untracked: &[git::Untracked], patches: &mut HashMap<String, File>) -> File {
    let loaded = if file.status == Status::Untracked {
        untracked.iter().find(|u| u.path == file.path).map(|u| diff::untracked(&u.path, u.size, u.bytes.as_deref()))
    } else {
        patches.remove(&file.path)
    };
    match loaded {
        Some(mut loaded) => {
            loaded.viewed_digest = file.viewed_digest;
            loaded.staged = file.staged;
            loaded
        }
        None => file,
    }
}

struct Fetched {
    numstat: Vec<u8>,
    names: Vec<u8>,
    untracked: Vec<git::Untracked>,
}

struct ParsedNames {
    statuses: Statuses,
    renames: HashMap<String, String>,
}

fn compared(request: &Request, expanded: &[String]) -> Result<Loaded> {
    let dir = request.dir.as_path();
    let (base, from) = git::comparison(request, dir)?;
    let to = (request.mode == Mode::Commits).then_some("HEAD");
    let fetched = fetch_range(dir, &from, to, request.mode)?;
    let parsed = names(&fetched.names);
    let previous = previous_of(request);
    let paths = patch_paths(expanded, &fetched.untracked, &parsed.renames, &previous);
    let patch = ranged_patch(dir, &from, to, &paths)?;
    let digest = digest_of(
        dir,
        &parsed.statuses,
        (&request.mode, &from, &fetched.numstat, &fetched.names, &fetched.untracked, expanded, &patch),
    );
    if request.last == Some(digest) {
        return Ok(Loaded { base, digest, diff: None });
    }
    let files = ranged_files(&from, &fetched, &parsed, (&patch, expanded, &previous));
    Ok(Loaded { base, digest, diff: Some(Diff { files }) })
}

fn fetch_range(dir: &Path, from: &str, to: Option<&str>, mode: Mode) -> Result<Fetched> {
    Ok(Fetched {
        numstat: stat(dir, from, to, "--numstat")?,
        names: stat(dir, from, to, "--name-status")?,
        untracked: if mode == Mode::All { git::untracked(dir)? } else { Vec::new() },
    })
}

fn stat(dir: &Path, from: &str, to: Option<&str>, flag: &str) -> Result<Vec<u8>> {
    let mut args = vec!["diff", flag, "-z", "--find-renames", "--relative", "--no-ext-diff", "--no-textconv", from];
    if let Some(to) = to {
        args.push(to);
    }
    args.extend(["--", "."]);
    git::run(dir, &args)
}

fn ranged_patch(dir: &Path, from: &str, to: Option<&str>, paths: &[&str]) -> Result<Vec<u8>> {
    if paths.is_empty() { Ok(Vec::new()) } else { git::patch_between(dir, from, to, paths) }
}

fn names(raw: &[u8]) -> ParsedNames {
    let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
    let mut parsed = ParsedNames { statuses: HashMap::new(), renames: HashMap::new() };
    while let Some(code) = parts.next() {
        push_name(&mut parsed, &mut parts, &String::from_utf8_lossy(code));
    }
    parsed
}

fn push_name<'a>(parsed: &mut ParsedNames, parts: &mut impl Iterator<Item = &'a [u8]>, code: &str) {
    let Some(path) = parts.next() else { return };
    let path = String::from_utf8_lossy(path).into_owned();
    if letter(code) != Status::Renamed {
        parsed.statuses.insert(path, letter(code));
        return;
    }
    let Some(new) = parts.next() else { return };
    let new = String::from_utf8_lossy(new).into_owned();
    parsed.renames.insert(new.clone(), path);
    parsed.statuses.insert(new, Status::Renamed);
}

fn letter(code: &str) -> Status {
    match code.as_bytes().first() {
        Some(b'A') => Status::Added,
        Some(b'D') => Status::Deleted,
        Some(b'R' | b'C') => Status::Renamed,
        _ => Status::Modified,
    }
}

fn previous_of(request: &Request) -> Previous<'_> {
    request.previous.iter().map(|file| (file.path.as_str(), file)).collect()
}

fn ranged_files(
    from: &str,
    fetched: &Fetched,
    parsed: &ParsedNames,
    rest: (&[u8], &[String], &Previous<'_>),
) -> Vec<Arc<File>> {
    let (patch, expanded, previous) = rest;
    let mut files = counted(&fetched.numstat, &parsed.statuses);
    files.extend(
        fetched.untracked.iter().map(|file| diff::untracked_file(&file.path, file.size, file.bytes.as_deref(), false)),
    );
    for file in &mut files {
        file.digest = file_digest(from, file);
        file.staged = None;
    }
    let mut patches = parsed_patches(patch);
    let mut files: Vec<_> =
        files.into_iter().map(|file| materialize(file, expanded, &fetched.untracked, &mut patches, previous)).collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn parsed_patches(patch: &[u8]) -> HashMap<String, File> {
    diff::parse(&String::from_utf8_lossy(patch)).into_iter().map(|file| (file.path.clone(), file)).collect()
}

fn materialize(
    file: File,
    expanded: &[String],
    untracked: &[git::Untracked],
    patches: &mut HashMap<String, File>,
    previous: &Previous<'_>,
) -> Arc<File> {
    let file = if expanded.contains(&file.path) { opened(file, untracked, patches) } else { file };
    git::reuse(file, previous)
}

fn file_digest(from: &str, file: &File) -> u64 {
    let mut hasher = DefaultHasher::new();
    (from, &file.path, &file.old_path, file.status, file.added, file.removed).hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests;
