use std::collections::HashMap;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const BLOCK: u64 = 512;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) bytes: u64,
    pub(crate) parts: Vec<u64>,
    pub(crate) nested: Vec<PathBuf>,
}

struct Links {
    blocks: u64,
    count: u64,
    seen: u64,
    part: Option<usize>,
    mixed: bool,
}

struct Walk<'a> {
    device: u64,
    parts: &'a [PathBuf],
    usage: Usage,
    links: HashMap<(u64, u64), Links>,
    unreadable: usize,
}

pub(crate) fn usage(root: &Path, parts: &[PathBuf]) -> Usage {
    let Ok(meta) = std::fs::symlink_metadata(root) else {
        return Usage { parts: vec![0; parts.len()], ..Usage::default() };
    };
    let mut walk = Walk {
        device: meta.dev(),
        parts,
        usage: Usage { parts: vec![0; parts.len()], ..Usage::default() },
        links: HashMap::new(),
        unreadable: 0,
    };
    walk.add_blocks(&meta, None);
    walk.dir(root, Path::new(""), None);
    walk.settle();
    if walk.unreadable > 0 {
        crate::log::debug!("worktrees", "some folders could not be read", count = walk.unreadable);
    }
    walk.usage
}

impl Walk<'_> {
    fn add_blocks(&mut self, meta: &Metadata, part: Option<usize>) {
        let bytes = meta.blocks() * BLOCK;
        self.usage.bytes += bytes;
        if let Some(part) = part {
            self.usage.parts[part] += bytes;
        }
    }

    fn dir(&mut self, path: &Path, relative: &Path, part: Option<usize>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            self.unreadable += 1;
            return;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let child = relative.join(entry.file_name());
            let inside = part.or_else(|| self.parts.iter().position(|p| *p == child));
            if inside.is_some() && entry.file_name() == ".git" {
                self.usage.nested.push(child.clone());
            }
            if meta.is_dir() {
                self.subdir(&entry.path(), &child, (&meta, inside));
            } else {
                self.file(&meta, inside);
            }
        }
    }

    fn subdir(&mut self, path: &Path, child: &Path, (meta, inside): (&Metadata, Option<usize>)) {
        if meta.dev() == self.device {
            self.add_blocks(meta, inside);
            self.dir(path, child, inside);
        }
    }

    fn file(&mut self, meta: &Metadata, part: Option<usize>) {
        if meta.dev() != self.device {
            return;
        }
        if meta.nlink() <= 1 || meta.is_symlink() {
            self.add_blocks(meta, part);
            return;
        }
        let links = self.links.entry((meta.dev(), meta.ino())).or_insert(Links {
            blocks: meta.blocks(),
            count: meta.nlink(),
            seen: 0,
            part,
            mixed: false,
        });
        links.seen += 1;
        links.mixed |= links.part != part;
    }

    fn settle(&mut self) {
        for links in self.links.values().filter(|links| links.seen >= links.count) {
            let bytes = links.blocks * BLOCK;
            self.usage.bytes += bytes;
            if let (Some(part), false) = (links.part, links.mixed) {
                self.usage.parts[part] += bytes;
            }
        }
    }
}
