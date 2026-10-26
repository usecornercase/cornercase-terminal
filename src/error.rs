use std::path::PathBuf;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub struct Workspaces {
    pub handle: String,
    pub choices: Vec<(String, String)>,
    pub message: String,
}

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
    #[error("cannot listen on `{}`: {cause}. {}", path.display(), listen_advice(*custom))]
    Listen { path: PathBuf, cause: std::io::Error, custom: bool },
    #[error(
        "the cornercase socket `{}` is {length} bytes long, more than the {max} a socket path may have. {}",
        path.display(),
        too_long_advice(*custom)
    )]
    SocketTooLong { path: PathBuf, length: usize, max: usize, custom: bool },
    #[error(
        "a cornercase server holds the lock next to `{}` but does not answer there: it may be stuck, or run on \
         another machine that shares this home folder. Stop it, or set CORNERCASE_SOCKET to a socket on this \
         machine's disk {KEEPS_NEXT_TO_IT}",
        .0.display()
    )]
    ServerUnreachable(PathBuf),
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
    #[error("failed to run glab: {0}")]
    RunGlab(std::io::Error),
    #[error("{0}")]
    Glab(String),
    #[error("gh printed something unexpected: {0}")]
    GhOutput(serde_json::Error),
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    TokenRejected(String),
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Plugin(String),
    #[error("{0}")]
    Account(String),
    #[error("{}", .0.message)]
    Workspaces(Box<Workspaces>),
    #[error("cannot read `{}`: {reason}", path.display())]
    CodeWorkspace { path: PathBuf, reason: String },
    #[error("cannot read `{}`: {cause}", path.display())]
    Record { path: PathBuf, cause: String },
    #[error("cannot use the flow `{}`: {reason}", path.display())]
    FlowFile { path: PathBuf, reason: String },
    #[error("`{0}` already exists")]
    PathExists(PathBuf),
    #[error("no cornercase server is running")]
    NoServer,
    #[error("{0}")]
    Control(String),
    #[error("{0}")]
    WrongUsage(String),
    #[error("the answer is printed, but part of it failed")]
    Partial,
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
        crate::version::CURRENT
    )]
    RemoteOlder { host: String, there: String },
    #[error(
        "`{host}` runs cornercase {there} and this machine {}. Run `cornercase update` here",
        crate::version::CURRENT
    )]
    RemoteNewer { host: String, there: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

const KEEPS_NEXT_TO_IT: &str = "(cornercase then keeps its settings, session and log next to it)";

fn unsafe_dir_advice(custom: bool) -> String {
    if custom {
        "Point CORNERCASE_SOCKET at a socket in a folder only you can write to".into()
    } else {
        format!(
            "cornercase keeps its session there, so it must be a real folder that belongs to you: fix it, or set \
             CORNERCASE_SOCKET to a socket in a folder only you can write to {KEEPS_NEXT_TO_IT}"
        )
    }
}

fn too_long_advice(custom: bool) -> String {
    if custom {
        "Point CORNERCASE_SOCKET at a shorter path".into()
    } else {
        format!(
            "Set XDG_STATE_HOME to a shorter folder, or CORNERCASE_SOCKET to a shorter socket path {KEEPS_NEXT_TO_IT}"
        )
    }
}

fn listen_advice(custom: bool) -> String {
    if custom {
        "Point CORNERCASE_SOCKET at a socket on a local disk".into()
    } else {
        format!(
            "Its folder may be on a network disk that cannot hold a socket: set XDG_STATE_HOME to a folder on a \
             local disk, or CORNERCASE_SOCKET to a socket on one {KEEPS_NEXT_TO_IT}"
        )
    }
}
