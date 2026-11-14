use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(20);

pub(crate) fn run(program: &Path, args: &[String], out: &mut impl Write, err: &mut impl Write) -> i32 {
    record(program, args);
    if program.with_extension("echo").exists() {
        echo(std::io::stdin().lock(), out);
    }
    copy(&program.with_extension("out"), out);
    copy(&program.with_extension("err"), err);
    let _listener = match listen(program, args) {
        Ok(listener) => listener,
        Err(message) => {
            let _ = writeln!(err, "{message}");
            return 255;
        }
    };
    hold(program);
    exit_code(program)
}

fn record(program: &Path, args: &[String]) {
    append(&program.with_extension("args"), &format!("{}\n", args.join(" ")));
    append(&program.with_extension("argv"), &format!("{args:?}\n"));
    let folder = std::env::current_dir().map(|dir| dir.display().to_string()).unwrap_or_default();
    append(&program.with_extension("cwd"), &format!("{folder}\n"));
    append(&program.with_extension("pid"), &format!("{}\n", std::process::id()));
}

fn echo(mut input: impl Read, to: &mut impl Write) {
    let mut buffer = [0u8; 4096];
    while let Ok(read) = input.read(&mut buffer) {
        if read == 0 || to.write_all(&buffer[..read]).and_then(|()| to.flush()).is_err() {
            return;
        }
    }
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

fn listen(program: &Path, args: &[String]) -> Result<Option<TcpListener>, String> {
    if !program.with_extension("forward").exists() {
        return Ok(None);
    }
    let spec = args.iter().position(|arg| arg == "-L").and_then(|at| args.get(at + 1));
    let mut fields = spec.map(|spec| spec.split(':')).into_iter().flatten();
    let (Some(address), Some(local)) = (fields.next(), fields.next().and_then(|local| local.parse::<u16>().ok()))
    else {
        return Err("no -L forward to listen on".into());
    };
    TcpListener::bind((address, local)).map(Some).map_err(|error| format!("bind {address}:{local}: {error}"))
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
