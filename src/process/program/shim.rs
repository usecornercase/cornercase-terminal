use std::io::Read;
use std::path::{Path, PathBuf};

const LIMIT: u64 = 4096;
const HEAD: &str =
    "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n";
const NODE: &str = "\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \
                    \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL \
                    || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\";
const NODE_END: &str = "\" %*\r\n";
const ITSELF: &str = "\"%dp0%\\";
const ITSELF_END: &str = "\"   %*\r\n";
const SHAPES: [(Runs, &str, &str); 2] = [(Runs::Node, NODE, NODE_END), (Runs::Itself, ITSELF, ITSELF_END)];
const NODE_EXE: &str = "node.exe";
const UNREAD: [char; 4] = ['"', '%', ':', '/'];

#[derive(Clone, Copy)]
enum Runs {
    Node,
    Itself,
}

pub(crate) fn target(shim: &Path, find: impl Fn(&str) -> Option<PathBuf>) -> Option<Vec<PathBuf>> {
    let text = read(shim)?;
    let body = text.strip_prefix(HEAD)?;
    let (runs, relative) =
        SHAPES.iter().find_map(|(runs, start, end)| Some((*runs, body.strip_prefix(start)?.strip_suffix(end)?)))?;
    let dir = shim.parent()?;
    let script = joined(dir, relative).filter(|script| script.is_file())?;
    match runs {
        Runs::Node => Some(vec![node(dir, find)?, script]),
        Runs::Itself => is_exe(&script).then(|| vec![script]),
    }
}

fn read(shim: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(shim).ok()?.take(LIMIT).read_to_string(&mut text).ok()?;
    Some(text)
}

fn joined(dir: &Path, relative: &str) -> Option<PathBuf> {
    relative.split('\\').try_fold(dir.to_path_buf(), |mut path, part| match part {
        ".." => path.pop().then_some(path),
        _ if plain(part) => Some(path.join(part)),
        _ => None,
    })
}

fn plain(part: &str) -> bool {
    !part.is_empty() && part != "." && !part.chars().any(|c| c.is_control() || UNREAD.contains(&c))
}

fn node(dir: &Path, find: impl Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    let beside = dir.join(NODE_EXE);
    if beside.is_file() {
        return Some(beside);
    }
    find("node").filter(|node| is_exe(node))
}

fn is_exe(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(test)]
mod tests;
