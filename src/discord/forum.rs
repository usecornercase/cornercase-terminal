use serde_json::{Value, json};

use super::{Bot, Call, Failure, MORE, application_id, message, pages, snowflake};
use crate::http::Method;
use crate::messenger::{Forum, Keyboard, Order, Tags};

const VIEW_CHANNEL: u64 = 1 << 10;
const MANAGE_CHANNELS: u64 = 1 << 4;
const SEND_MESSAGES: u64 = 1 << 11;
const SEND_IN_THREADS: u64 = 1 << 38;
const MANAGE_THREADS: u64 = 1 << 34;
const READ_HISTORY: u64 = 1 << 16;
const ADD_REACTIONS: u64 = 1 << 6;
const IN_FORUM: u64 = VIEW_CHANNEL | SEND_MESSAGES | SEND_IN_THREADS | MANAGE_THREADS | READ_HISTORY | ADD_REACTIONS;
pub(crate) const PERMISSIONS: u64 = IN_FORUM | MANAGE_CHANNELS;
const GUILD_INSTALL: &str = "0";
const MESSAGE_CONTENT: u64 = 1 << 18;
const MESSAGE_CONTENT_LIMITED: u64 = 1 << 19;
const FORUM: u64 = 15;
const ROLE: u64 = 0;
const MEMBER: u64 = 1;
const ARCHIVE_AFTER: u64 = 10_080;
pub(crate) const NAME: &str = "cornercase";
pub(crate) const WORKING: &str = "working";
pub(crate) const WAITING: &str = "needs you";
pub(crate) const DONE: &str = "done";
const TAGS: [&str; 3] = [WORKING, WAITING, DONE];
pub(super) const SOME_PERMISSION: &str = "one the bot was not given";
const MANAGE_CHANNELS_NAME: &str = "Manage Channels";
const UNREAD: Tags = Tags { working: 0, waiting: 0, done: 0 };

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Server {
    pub(crate) id: i64,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Checklist {
    pub(crate) guild_install: bool,
    pub(crate) content: bool,
    pub(crate) servers: Vec<Server>,
}

impl Checklist {
    pub(crate) fn pick(&self, chosen: Option<i64>) -> Option<&Server> {
        match self.servers.as_slice() {
            [only] => Some(only),
            servers => servers.iter().find(|server| Some(server.id) == chosen),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Progress {
    pub(crate) checklist: Checklist,
    pub(crate) forum: Option<Result<Forum, Failure>>,
}

pub(super) struct Post<'a> {
    pub(super) forum: i64,
    pub(super) title: &'a str,
    pub(super) tag_id: i64,
    pub(super) text: &'a str,
    pub(super) keyboard: &'a Keyboard,
}

pub(crate) fn guild_install_link(token: &str) -> Option<String> {
    application_id(token).map(|id| {
        format!(
            "https://discord.com/oauth2/authorize?client_id={id}&integration_type=0&scope=bot+applications.commands&permissions={PERMISSIONS}"
        )
    })
}

fn content_intent(app: &Value) -> bool {
    app["flags"].as_u64().unwrap_or_default() & (MESSAGE_CONTENT | MESSAGE_CONTENT_LIMITED) != 0
}

pub(super) fn posting(post: &Post<'_>, thread: Option<i64>) -> Vec<Call> {
    let mut pages = pages(post.keyboard).into_iter();
    let first = json!({
        "name": post.title,
        "applied_tags": [post.tag_id.to_string()],
        "message": message(post.text, &pages.next().unwrap_or_default()),
    });
    let mut calls = vec![Call::new(Method::Post, format!("/channels/{}/threads", post.forum), Some(first))];
    if let Some(thread) = thread {
        let path = format!("/channels/{thread}/messages");
        calls.extend(pages.map(|page| Call::new(Method::Post, path.clone(), Some(message(MORE, &page)))));
    }
    calls
}

pub(super) fn retagging(thread: i64, tag_id: i64) -> Call {
    Call::new(Method::Patch, format!("/channels/{thread}"), Some(json!({ "applied_tags": [tag_id.to_string()] })))
}

pub(super) fn archiving(thread: i64, archived: bool, locked: bool) -> Call {
    Call::new(Method::Patch, format!("/channels/{thread}"), Some(json!({ "archived": archived, "locked": locked })))
}

pub(super) fn removing(thread: i64) -> Call {
    Call::new(Method::Delete, format!("/channels/{thread}"), None)
}

pub(super) fn needs(failure: Failure, order: &Order) -> Failure {
    let permission = match order {
        Order::Post { .. } => "Send Messages",
        Order::Retag { .. } | Order::Close { .. } | Order::Reopen { .. } | Order::Remove { .. } => "Manage Threads",
        Order::Send { .. } => "Send Messages in Threads",
        Order::React { .. } => "Add Reactions",
        _ => return failure,
    };
    missing(failure, permission)
}

fn missing(failure: Failure, permission: &'static str) -> Failure {
    match failure {
        Failure::Missing(_) => Failure::Missing(permission),
        failure => failure,
    }
}

fn creating(guild: i64, bot: &str) -> Value {
    let tags: Vec<Value> = TAGS.iter().map(|name| json!({ "name": name })).collect();
    json!({
        "name": NAME,
        "type": FORUM,
        "default_auto_archive_duration": ARCHIVE_AFTER,
        "available_tags": tags,
        "permission_overwrites": [
            { "id": guild.to_string(), "type": ROLE, "deny": VIEW_CHANNEL.to_string() },
            { "id": bot, "type": MEMBER, "allow": IN_FORUM.to_string() },
        ],
    })
}

fn listed(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn tag(tags: &[Value], name: &str) -> Option<i64> {
    tags.iter().find(|tag| tag["name"].as_str() == Some(name)).and_then(|tag| snowflake(&tag["id"]))
}

fn tags_of(channel: &Value) -> Option<Tags> {
    let tags = listed(&channel["available_tags"]);
    Some(Tags { working: tag(tags, WORKING)?, waiting: tag(tags, WAITING)?, done: tag(tags, DONE)? })
}

fn with_tags(forum: &Forum, channel: &Value) -> Result<Forum, Failure> {
    let tags = tags_of(channel).ok_or_else(|| Failure::Refused("the forum has no tags".into()))?;
    Ok(Forum { tags, ..forum.clone() })
}

impl Bot {
    pub(crate) fn set_up(&self, chosen: Option<i64>, saved: Option<&Forum>) -> Result<Progress, Failure> {
        let checklist = self.checklist()?;
        let picked = checklist.pick(chosen).filter(|_| checklist.content);
        let forum = picked.map(|server| self.forum(server, saved));
        Ok(Progress { checklist, forum })
    }

    pub(crate) fn checklist(&self) -> Result<Checklist, Failure> {
        let app = self.app()?;
        let (owner, _) = Self::owner(&app)?;
        let guild_install = !app["integration_types_config"][GUILD_INSTALL].is_null();
        let servers = if guild_install { self.servers_of(owner)? } else { Vec::new() };
        Ok(Checklist { guild_install, content: content_intent(&app), servers })
    }

    fn servers_of(&self, owner: i64) -> Result<Vec<Server>, Failure> {
        let guilds = self.call(Method::Get, "/users/@me/guilds", None)?;
        listed(&guilds)
            .iter()
            .filter_map(|guild| snowflake(&guild["id"]).map(|id| (id, guild["name"].as_str().unwrap_or_default())))
            .map(|(id, name)| {
                let guild = self.call(Method::Get, &format!("/guilds/{id}"), None)?;
                let theirs = snowflake(&guild["owner_id"]) == Some(owner);
                Ok(theirs.then(|| Server { id, name: name.to_string() }))
            })
            .filter_map(Result::transpose)
            .collect()
    }

    pub(crate) fn forum(&self, server: &Server, saved: Option<&Forum>) -> Result<Forum, Failure> {
        let kept = saved.filter(|forum| forum.guild == server.id).map(|forum| self.ensure_tags(forum));
        match kept {
            Some(Err(Failure::Lost)) | None => match self.found(server)? {
                Some(forum) => self.ensure_tags(&forum),
                None => self.create_forum(server),
            },
            Some(done) => done,
        }
    }

    fn found(&self, server: &Server) -> Result<Option<Forum>, Failure> {
        let channels = self.call(Method::Get, &format!("/guilds/{}/channels", server.id), None)?;
        let found = listed(&channels)
            .iter()
            .find(|channel| channel["type"].as_u64() == Some(FORUM) && channel["name"].as_str() == Some(NAME));
        let channel = found.and_then(|channel| snowflake(&channel["id"]));
        Ok(channel.map(|channel| Forum { guild: server.id, guild_name: server.name.clone(), channel, tags: UNREAD }))
    }

    pub(crate) fn create_forum(&self, server: &Server) -> Result<Forum, Failure> {
        let bot = self.application()?;
        let path = format!("/guilds/{}/channels", server.id);
        let channel = self
            .call(Method::Post, &path, Some(&creating(server.id, &bot)))
            .map_err(|f| missing(f, MANAGE_CHANNELS_NAME))?;
        let id = snowflake(&channel["id"]).ok_or_else(|| Failure::Refused("the forum has no id".into()))?;
        with_tags(&Forum { guild: server.id, guild_name: server.name.clone(), channel: id, tags: UNREAD }, &channel)
    }

    pub(crate) fn ensure_tags(&self, forum: &Forum) -> Result<Forum, Failure> {
        let path = format!("/channels/{}", forum.channel);
        let channel = self.call(Method::Get, &path, None)?;
        if tags_of(&channel).is_some() {
            return with_tags(forum, &channel);
        }
        let mut tags = listed(&channel["available_tags"]).to_vec();
        let wanting: Vec<&str> = TAGS.into_iter().filter(|name| tag(&tags, name).is_none()).collect();
        tags.extend(wanting.into_iter().map(|name| json!({ "name": name })));
        let body = json!({ "available_tags": tags });
        let patched = self.call(Method::Patch, &path, Some(&body)).map_err(|f| missing(f, MANAGE_CHANNELS_NAME))?;
        with_tags(forum, &patched)
    }
}

#[cfg(test)]
mod tests;
