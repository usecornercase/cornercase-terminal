use super::{Dialog, Done, Offer, Panel, Step, Target, Tour};

const fn step(id: &'static str, title: &'static str, text: &'static str, target: Target, done: Done) -> Step {
    Step { id, title, text, target, done, offer: None }
}

const fn offer(step: Step, offer: Offer) -> Step {
    Step { offer: Some(offer), ..step }
}

const fn panel(panel: Panel) -> Done {
    Done::Panel { panel }
}

const fn opened(dialog: Dialog) -> Done {
    Done::Opened { dialog }
}

const CORE: [Step; 7] = [
    step(
        "projects",
        "Projects",
        "A project is a folder you work in. Open one with + New, then open project. If cornercase started in a \
         folder, that folder is already your first project.",
        Target::NewProject,
        Done::ProjectOpened,
    ),
    step(
        "workspaces",
        "Workspaces",
        "A workspace is a line of work. In a git repository each one can get its own worktree, so several agents \
         change code side by side without touching each other's files. Make one with + New, then new workspace. \
         Outside a repository the default workspace is all you need.",
        Target::Workspaces,
        Done::WorkspaceMade,
    ),
    step(
        "tabs",
        "Tabs and panes",
        "Each tab runs shells. Open one with + Tab, or right-click a pane for its menu: split it, or close it. \
         Drag a border to resize.",
        Target::Tabs,
        Done::TabOpened,
    ),
    offer(
        step(
            "prompt",
            "Your first prompt",
            "Start an agent in this workspace with a first prompt. Edit the idea below, then start it. If the agent \
             asks whether to trust the folder, that answer is yours.",
            Target::Pane,
            Done::PromptConfirmed,
        ),
        Offer::Prompt,
    ),
    offer(
        step(
            "status",
            "Agents and their status",
            "A tab shows what its agent does: ◐ working, ! needs you, ✓ finished, ○ idle, ● marked for review. \
             The agents section lists every agent in every project; a click jumps to the one that needs you.",
            Target::Agents,
            Done::Next,
        ),
        Offer::AgentsSection,
    ),
    offer(
        step(
            "settings",
            "Essential settings",
            "In Settings: the default agent and its mode, resuming agents after a restart, desktop notifications \
             and sound, the working icon and the icon bar. Nothing changes until you change it.",
            Target::Settings,
            Done::Next,
        ),
        Offer::Settings,
    ),
    step(
        "next",
        "Where to go next",
        "Help, at the bottom of the sidebar, opens this tour again, a catalogue of short tours for the rest of \
         cornercase, and the docs.",
        Target::Help,
        Done::Next,
    ),
];

const ISSUES: [Step; 3] = [
    step(
        "open",
        "Issues",
        "Issues lists the issues of GitHub, GitLab, Linear, Jira and more. Open it.",
        Target::Issues,
        opened(Dialog::Issues),
    ),
    step(
        "start",
        "Start an agent on one",
        "Pick an issue and press start: cornercase makes a worktree on its branch and starts an agent with the \
         issue as its prompt.",
        Target::Issues,
        Done::Next,
    ),
    step(
        "schedule",
        "Later",
        "Schedule puts the issue in the queue, to start at a time you choose.",
        Target::Issues,
        Done::Next,
    ),
];

const CHANGES: [Step; 3] = [
    step(
        "open",
        "Changes",
        "Changes shows the diff of this workspace. Open it.",
        Target::Changes,
        panel(Panel::Changes),
    ),
    step(
        "ask",
        "Ask the agent",
        "Hover a hunk: ask agent pastes its place into the workspace's agent, so you can say what to fix.",
        Target::Changes,
        Done::Next,
    ),
    step(
        "viewed",
        "Viewed",
        "Mark a file viewed to fold it; it opens again when it changes. Commits and All compare against a branch.",
        Target::Changes,
        Done::Next,
    ),
];

const FILES: [Step; 3] = [
    step("open", "Files", "Files shows the workspace's files. Open it.", Target::Files, panel(Panel::Files)),
    step(
        "search",
        "Search",
        "Type in its bar to search the text in the files; the button at its right searches names.",
        Target::Files,
        Done::Next,
    ),
    step(
        "paths",
        "Paths in panes",
        "A path an agent prints is a link: click it to open the file at its line.",
        Target::Pane,
        Done::Next,
    ),
];

const FLOWS: [Step; 3] = [
    step(
        "what",
        "Flows",
        "A flow is a team of agents and the plan they follow: one codes, others review, an orchestrator runs it.",
        Target::Workspaces,
        Done::Next,
    ),
    step(
        "start",
        "Start one",
        "In the workspaces column, + New, then start a flow. The team works in a new worktree.",
        Target::Workspaces,
        Done::Next,
    ),
    step(
        "edit",
        "Your own",
        "Settings, Flows lists the built-in flows; edit one or make your own.",
        Target::Settings,
        Done::Next,
    ),
];

const TODO: [Step; 3] = [
    step("open", "TODO", "TODO keeps a list of what is next. Open it.", Target::Todo, panel(Panel::Todo)),
    step("add", "Add an item", "Click + New TODO and type one.", Target::Todo, Done::TodoAdded),
    step(
        "send",
        "Send it to an agent",
        "The ▸ of an item sends its text to an agent as a prompt, and checks it off once the agent takes it.",
        Target::Todo,
        Done::Next,
    ),
];

const QUEUE: [Step; 3] = [
    step(
        "open",
        "Queue",
        "The queue starts agents at a time you choose. It is the last tab of Issues; open Issues.",
        Target::Issues,
        opened(Dialog::Issues),
    ),
    step(
        "new",
        "Schedule",
        "In the Queue tab, new asks for a prompt, a time and a place: in two hours, tomorrow at 9, or when your plan resets.",
        Target::Issues,
        Done::Next,
    ),
    step(
        "run",
        "While you are away",
        "Entries start while the server runs, with or without a window.",
        Target::None,
        Done::Next,
    ),
];

const SEARCH: [Step; 3] = [
    step(
        "open",
        "Search",
        "The search bar finds projects, workspaces and tabs by name. Click it.",
        Target::Search,
        opened(Dialog::Search),
    ),
    step(
        "said",
        "In sessions",
        "Tab, or the button at the bar's right, searches what was said in agents and panes instead.",
        Target::Search,
        Done::Next,
    ),
    step(
        "go",
        "Go there",
        "Click a result, or press Enter: cornercase shows its pane and selects what matched.",
        Target::Search,
        Done::Next,
    ),
];

const PHONE: [Step; 3] = [
    step(
        "turn",
        "Your phone's browser",
        "Turn on browser access in Settings, Remote. It goes through Tailscale, so only your devices reach it.",
        Target::Settings,
        Done::Next,
    ),
    step(
        "pair",
        "Pair a phone",
        "Remote, then open in your phone's browser, shows a code to scan.",
        Target::Remote,
        Done::Next,
    ),
    step(
        "use",
        "Use it",
        "The phone shows the same window: answer an agent, look at a diff, start one.",
        Target::None,
        Done::Next,
    ),
];

const CHATS: [Step; 3] = [
    step(
        "bot",
        "Telegram and Discord",
        "A bot of your own tells you when an agent needs you or finished. Set it up in Settings, Remote.",
        Target::Settings,
        Done::Next,
    ),
    step(
        "away",
        "Away",
        "Remote, then continue on Telegram or Discord, turns messages on until you turn them off.",
        Target::Remote,
        Done::Next,
    ),
    step(
        "reply",
        "Reply",
        "A reply goes to the agent the message was about; buttons answer its questions.",
        Target::None,
        Done::Next,
    ),
];

const ACCOUNTS: [Step; 3] = [
    step(
        "add",
        "Accounts",
        "Two logins for one agent, work and personal: add an account in Settings, Accounts.",
        Target::Settings,
        Done::Next,
    ),
    step(
        "switch",
        "Switch",
        "The bar at the bottom switches the account new tabs use; open tabs keep theirs.",
        Target::None,
        Done::Next,
    ),
    step(
        "start",
        "Per start",
        "Issues, flows and the queue can each start with another account.",
        Target::None,
        Done::Next,
    ),
];

const KEYS: [Step; 3] = [
    step(
        "prefix",
        "The prefix key",
        "cornercase takes no keys from your programs, unless you set a prefix key in Settings, UI.",
        Target::Settings,
        Done::Next,
    ),
    step(
        "menu",
        "The keys menu",
        "The prefix opens a menu of keys: tabs, panes, panels, search. It lists them, so there is nothing to learn.",
        Target::None,
        opened(Dialog::Keys),
    ),
    step("help", "Help", "h in the keys menu opens Help.", Target::Help, Done::Next),
];

const SCRIPTS: [Step; 3] = [
    step(
        "commands",
        "Commands",
        "Every pane can drive cornercase: cornercase status, start, send, wait and read. Try cornercase --help.",
        Target::Pane,
        Done::Next,
    ),
    step(
        "skill",
        "Agents that run agents",
        "cornercase skill prints a skill that teaches an agent those commands, so it can start and coordinate others.",
        Target::Pane,
        Done::Next,
    ),
    step(
        "events",
        "Follow",
        "cornercase events and status --follow stream what happens, for scripts and other UIs.",
        Target::None,
        Done::Next,
    ),
];

const PLUGINS: [Step; 3] = [
    step(
        "find",
        "Plugins",
        "Plugins add actions to menus, hooks on events and services. Find them at usecornercase.dev/plugins.",
        Target::None,
        Done::Next,
    ),
    step(
        "install",
        "Install",
        "cornercase plugin install owner/repo shows what it runs and installs it turned off.",
        Target::Pane,
        Done::Next,
    ),
    step("enable", "Turn it on", "Settings, Plugins turns one on.", Target::Settings, Done::Next),
];

const fn tour(id: &'static str, title: &'static str, docs: &'static str, steps: &'static [Step]) -> Tour {
    Tour { id, title, docs, steps }
}

pub(crate) const TOURS: [Tour; 14] = [
    tour(super::CORE, "Get started", "first-steps/", &CORE),
    tour("issues", "Issues to agents", "guides/issues-and-agents/", &ISSUES),
    tour("changes", "Reviewing changes", "guides/changes/", &CHANGES),
    tour("files", "Files and paths", "guides/files/", &FILES),
    tour("flows", "Flows", "guides/flows/", &FLOWS),
    tour("todo", "TODO", "guides/todo/", &TODO),
    tour("queue", "The queue", "guides/queue/", &QUEUE),
    tour("search", "Search", "guides/search/", &SEARCH),
    tour("phone", "Phone and browser", "guides/phone/", &PHONE),
    tour("chats", "Telegram and Discord", "guides/telegram/", &CHATS),
    tour("accounts", "Accounts", "guides/accounts/", &ACCOUNTS),
    tour("keys", "Keys", "reference/keyboard-and-mouse/", &KEYS),
    tour("scripts", "Scripts and agents", "guides/scripts-and-agents/", &SCRIPTS),
    tour("plugins", "Plugins", "guides/plugins/", &PLUGINS),
];
