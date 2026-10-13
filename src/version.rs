use std::time::UNIX_EPOCH;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn build_id() -> String {
    std::env::current_exe()
        .and_then(|exe| exe.metadata())
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(|| "unknown".into(), |since| since.as_nanos().to_string())
}

pub fn newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

fn parse(version: &str) -> Option<(u64, u64, u64, bool)> {
    let version = version.trim().trim_start_matches('v');
    let (core, pre) = version.split_once('-').map_or((version, None), |(core, pre)| (core, Some(pre)));
    let mut parts = core.split('+').next()?.split('.').map(|p| p.parse::<u64>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some((major, minor, patch, pre.is_none()))
}

#[cfg(test)]
mod tests;
