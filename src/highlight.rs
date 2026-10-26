mod limit;
pub(crate) mod markdown;
pub(crate) mod syntax;
mod worker;

pub(crate) fn worker_main() -> i32 {
    match worker::serve() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
