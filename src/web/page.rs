use super::http::Response;

pub const INDEX: &str = include_str!("assets/index.html");
pub const NOT_PAIRED: &str = include_str!("assets/not-paired.html");
pub const EXPIRED: &str = include_str!("assets/expired.html");
pub const OTHER_USER: &str = include_str!("assets/other-user.html");

const ASSETS: &[(&str, &str, &str)] = &[
    ("/app.js", "text/javascript; charset=utf-8", include_str!("assets/app.js")),
    ("/app.css", "text/css; charset=utf-8", include_str!("assets/app.css")),
    ("/xterm.js", "text/javascript; charset=utf-8", include_str!("assets/xterm.js")),
    ("/xterm.css", "text/css; charset=utf-8", include_str!("assets/xterm.css")),
    ("/addon-fit.js", "text/javascript; charset=utf-8", include_str!("assets/addon-fit.js")),
    ("/xterm-LICENSE.txt", "text/plain; charset=utf-8", include_str!("assets/xterm-LICENSE.txt")),
];

pub fn asset(path: &str) -> Option<Response> {
    ASSETS.iter().find(|(name, _, _)| *name == path).map(|(_, kind, body)| Response::new("200 OK", kind, *body))
}

#[cfg(test)]
mod tests;
