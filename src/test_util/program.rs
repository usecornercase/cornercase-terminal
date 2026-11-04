use std::io::Write;
use std::path::Path;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(20);

pub(crate) fn run(program: &Path, args: &[String], out: &mut impl Write, err: &mut impl Write) -> i32 {
    record(program, args);
    copy(&program.with_extension("out"), out);
    copy(&program.with_extension("err"), err);
    hold(program);
    exit_code(program)
}

fn record(program: &Path, args: &[String]) {
    append(&program.with_extension("args"), &format!("{}\n", args.join(" ")));
    append(&program.with_extension("argv"), &format!("{args:?}\n"));
}

fn append(path: &Path, line: &str) {
    let log = std::fs::OpenOptions::new().create(true).append(true).open(path);
    let _ = log.and_then(|mut log| log.write_all(line.as_bytes()));
}

fn copy(from: &Path, to: &mut impl Write) {
    if let Ok(bytes) = std::fs::read(from) {
        let _ = to.write_all(&bytes);
        let _ = to.flush();
    }
}

fn hold(program: &Path) {
    let hold = program.with_extension("hold");
    while hold.exists() {
        std::thread::sleep(POLL);
    }
}

fn exit_code(program: &Path) -> i32 {
    let code = std::fs::read_to_string(program.with_extension("code")).unwrap_or_default();
    code.trim().parse().unwrap_or(0)
}

#[cfg(test)]
mod tests;
