use std::path::PathBuf;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to open a pty")]
    OpenPty(#[source] BoxError),
    #[error("failed to spawn shell `{shell}` in `{}`: {cause}", dir.display())]
    SpawnShell { shell: String, dir: PathBuf, cause: BoxError },
    #[error("failed to attach to the pty")]
    AttachPty(#[source] BoxError),
    #[error("failed to create the terminal emulator")]
    Emulator(#[source] BoxError),
    #[error("failed to listen on `{path}`")]
    Listen {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("a cornercase server is already running on `{0}`")]
    ServerRunning(PathBuf),
    #[error("refusing to use `{}`: it {reason}. {}", path.display(), unsafe_dir_advice(*custom))]
    UnsafeSocketDir { path: PathBuf, reason: &'static str, custom: bool },
    #[error("the other end of the cornercase socket belongs to another user (uid {0})")]
    OtherUser(u32),
    #[error("failed to start the cornercase server; see `{log}`")]
    ServerStart {
        log: PathBuf,
        #[source]
        source: Option<std::io::Error>,
    },
    #[error("cornercase is already running in this terminal; nesting it is not supported")]
    Nested,
    #[error("{0}")]
    Rejected(String),
    #[error("this is a development build; `cornercase update` only updates released builds")]
    DevelopmentBuild,
    #[error("failed to register signal handlers")]
    Signals(#[source] std::io::Error),
    #[error("failed to run git: {0}")]
    RunGit(std::io::Error),
    #[error("{0}")]
    Git(String),
    #[error("failed to run gh: {0}")]
    RunGh(std::io::Error),
    #[error("{0}")]
    Gh(String),
    #[error("gh printed something unexpected: {0}")]
    GhOutput(serde_json::Error),
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    Usage(String),
    #[error("cannot read `{}`: {reason}", path.display())]
    CodeWorkspace { path: PathBuf, reason: String },
    #[error("cannot read `{}`: {cause}", path.display())]
    Record { path: PathBuf, cause: String },
    #[error("`{0}` already exists")]
    PathExists(PathBuf),
    #[error("no cornercase server is running")]
    NoServer,
    #[error("{0}")]
    Control(String),
    #[error("{0}")]
    WrongUsage(String),
    #[error(
        "the running cornercase server is too old for `cornercase {0}`. Restart it: run `cornercase kill-server` \
         (it closes all its terminals) and start cornercase again"
    )]
    OldServer(&'static str),
    #[error("the cornercase server stopped before answering")]
    ServerGone,
    #[error("cornercase hit a bug, see server.log")]
    Bug,
    #[error("failed to run ssh: {0}")]
    RunSsh(std::io::Error),
    #[error("could not reach cornercase on `{host}`: {reason}")]
    Ssh { host: String, reason: String },
    #[error("cornercase was not found on `{0}`. Install it there, or give its path with --command")]
    RemoteMissing(String),
    #[error("the cornercase on `{0}` is too old to attach to from another machine. Run `cornercase update` there")]
    RemoteTooOld(String),
    #[error(
        "`{host}` runs cornercase {there} and this machine {}. Run `cornercase update` there",
        crate::update::CURRENT
    )]
    RemoteOlder { host: String, there: String },
    #[error(
        "`{host}` runs cornercase {there} and this machine {}. Run `cornercase update` here",
        crate::update::CURRENT
    )]
    RemoteNewer { host: String, there: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

fn unsafe_dir_advice(custom: bool) -> &'static str {
    if custom {
        "Point CORNERCASE_SOCKET at a socket in a folder only you can write to"
    } else {
        "If that folder is yours, remove it and start cornercase again; \
         if not, set CORNERCASE_SOCKET to a socket in a folder only you can write to"
    }
}
