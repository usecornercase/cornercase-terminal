use std::collections::{HashMap, HashSet};

use super::super::{Delivery, Route, TAGS};
use super::tags::tag_of;
use crate::app::App;
use crate::messenger::{Forum, Order, Via};
use crate::project::{Hierarchy, TabAt};
use crate::text::truncate_right;

mod saved;

const TITLE: usize = 100;
pub(super) const FAILED: &str = "a post could not be made, so its messages went to the DM";

#[derive(Debug, Default)]
pub(super) struct Posts {
    threads: HashMap<u64, i64>,
    making: HashMap<u64, Making>,
    sent: HashMap<u64, u64>,
    archived: HashSet<u64>,
    resent: HashSet<u64>,
    pub(super) failed: bool,
}

#[derive(Debug)]
struct Making {
    tag: u64,
    orders: Vec<Order>,
    pointer: Option<Pointer>,
}

#[derive(Debug)]
pub(in crate::app::messenger) struct Pointer {
    pub(in crate::app::messenger) line: String,
    pub(in crate::app::messenger) message: i64,
    pub(in crate::app::messenger) route: Route,
}

struct NewPost {
    forum: i64,
    title: String,
    tag_id: i64,
}

fn to_chat(mut order: Order, to: i64) -> Order {
    if let Order::Send { chat, .. } = &mut order {
        *chat = to;
    }
    order
}

fn first_post(order: &Order, post: NewPost) -> Order {
    let NewPost { forum, title, tag_id } = post;
    match order {
        Order::Send { text, keyboard, tag, .. } => {
            Order::Post { forum, title, tag_id, text: text.clone(), keyboard: keyboard.clone(), tag: *tag }
        }
        other => other.clone(),
    }
}

impl Posts {
    fn route(&mut self, workspace: u64, order: Order, post: NewPost) -> Option<Order> {
        let Order::Send { tag: Some(tag), .. } = order else { return Some(order) };
        if let Some(making) = self.making.get_mut(&workspace) {
            making.orders.push(order);
            return None;
        }
        if let Some(&thread) = self.threads.get(&workspace) {
            self.track(tag, workspace);
            self.archived.remove(&workspace);
            return Some(to_chat(order, thread));
        }
        let first = first_post(&order, post);
        self.making.insert(workspace, Making { tag, orders: vec![order], pointer: None });
        Some(first)
    }

    fn track(&mut self, tag: u64, workspace: u64) {
        self.sent.insert(tag, workspace);
        let oldest = tag.saturating_sub(TAGS);
        self.sent.retain(|kept, _| *kept > oldest);
    }

    fn making_of(&self, tag: u64) -> Option<u64> {
        self.making.iter().find(|(_, making)| making.tag == tag).map(|(workspace, _)| *workspace)
    }

    fn posted(&mut self, tag: u64, thread: i64) -> (Vec<Order>, Option<(u64, Pointer)>) {
        let Some(making) = self.making_of(tag).and_then(|workspace| self.making.remove_entry(&workspace)) else {
            return (Vec::new(), None);
        };
        let (workspace, Making { orders, pointer, .. }) = making;
        self.threads.insert(workspace, thread);
        self.failed = false;
        let rest: Vec<Order> = orders.into_iter().skip(1).collect();
        for tag in rest.iter().filter_map(Order::tag) {
            self.track(tag, workspace);
        }
        let rest = rest.into_iter().map(|order| to_chat(order, thread)).collect();
        (rest, pointer.map(|pointer| (workspace, pointer)))
    }

    fn failed(&mut self, tag: u64) -> Vec<Order> {
        let Some(workspace) = self.making_of(tag) else { return Vec::new() };
        self.failed = true;
        self.making.remove(&workspace).map(|making| making.orders).unwrap_or_default()
    }

    fn gone(&mut self, thread: i64, tag: Option<u64>) -> Option<bool> {
        if let Some(workspace) = self.workspace_of(thread) {
            self.threads.remove(&workspace);
            self.archived.remove(&workspace);
        }
        let tag = tag.filter(|tag| self.sent.remove(tag).is_some())?;
        let oldest = tag.saturating_sub(TAGS);
        self.resent.retain(|kept| *kept > oldest);
        Some(self.resent.insert(tag))
    }

    pub(super) fn workspace_of(&self, thread: i64) -> Option<u64> {
        self.threads.iter().find(|(_, known)| **known == thread).map(|(workspace, _)| *workspace)
    }

    pub(super) fn thread(&self, workspace: u64) -> Option<i64> {
        self.threads.get(&workspace).copied()
    }

    pub(super) fn threads(&self) -> Vec<(u64, i64)> {
        self.threads.iter().map(|(workspace, thread)| (*workspace, *thread)).collect()
    }

    pub(super) fn archived(&self, workspace: u64) -> bool {
        self.archived.contains(&workspace)
    }

    pub(super) fn archive(&mut self, workspace: u64, archived: bool) {
        if archived {
            self.archived.insert(workspace);
        } else {
            self.archived.remove(&workspace);
        }
    }

    pub(super) fn forget_workspace(&mut self, workspace: u64) {
        self.threads.remove(&workspace);
        self.sent.retain(|_, sent| *sent != workspace);
        self.archived.remove(&workspace);
    }

    fn unmade(&mut self) -> Vec<Order> {
        self.making.drain().flat_map(|(_, making)| making.orders).collect()
    }

    pub(super) fn forget(&mut self) {
        self.threads.clear();
        self.sent.clear();
        self.archived.clear();
        self.resent.clear();
    }
}

impl App {
    pub(super) fn forum_on(&self, via: Via) -> Option<&Forum> {
        let messenger = self.messenger(via);
        let on = self.config.discord_forum && messenger.link.is_some() && messenger.forum_ready();
        messenger.saved.forum.as_ref().filter(|_| on)
    }

    pub(in crate::app::messenger) fn routed(&mut self, via: Via, route: Option<Route>, order: Order) -> Option<Order> {
        let (Some(Route::Pane(pane)), Order::Send { reply_to: None, .. }) = (route, &order) else {
            return Some(order);
        };
        let Some((forum, tags)) = self.forum_on(via).map(|forum| (forum.channel, forum.tags)) else {
            return Some(order);
        };
        let Some(at) = self.projects.tab_of_pane(pane).map(TabAt::workspace) else { return Some(order) };
        let tag_id = tag_of(self.projects.workspace(at), tags);
        let post = NewPost { forum, title: truncate_right(&self.place_name(at), TITLE), tag_id };
        let workspace = self.projects.workspace(at).id;
        let forum = &mut self.messenger_mut(via).forum;
        let routed = forum.posts.route(workspace, order, post);
        if matches!(routed, Some(Order::Post { .. })) {
            forum.labels.shown(workspace, tag_id);
        }
        routed
    }

    pub(in crate::app::messenger) fn post_workspace(&self, via: Via, route: Option<Route>) -> Option<u64> {
        let Some(Route::Pane(pane)) = route else { return None };
        self.forum_on(via)?;
        let at = self.projects.tab_of_pane(pane).map(TabAt::workspace)?;
        Some(self.projects.workspace(at).id)
    }

    pub(in crate::app::messenger) fn pointed(&mut self, via: Via, workspace: u64, pointer: Pointer) {
        let posts = &mut self.messenger_mut(via).forum.posts;
        if let Some(making) = posts.making.get_mut(&workspace) {
            making.pointer = Some(pointer);
        } else if posts.threads.contains_key(&workspace) {
            self.point_to_post(via, workspace, pointer);
        }
    }

    fn point_to_post(&mut self, via: Via, workspace: u64, pointer: Pointer) {
        let Some(post) = self.post_info(workspace) else { return };
        let Pointer { line, message, route } = pointer;
        let delivery = Delivery { route: Some(route), reply_to: Some(message), ..Delivery::default() };
        self.say(via, format!("{line} {}", post.url), delivery);
    }

    pub(in crate::app::messenger) fn knows(&self, via: Via, thread: i64) -> bool {
        self.messenger(via).forum.posts.workspace_of(thread).is_some()
    }

    pub(in crate::app::messenger) fn answered_in(&mut self, via: Via, order: Order) -> Order {
        let posts = &mut self.messenger_mut(via).forum.posts;
        if let Order::Send { chat, tag: Some(tag), .. } = &order
            && let Some(workspace) = posts.workspace_of(*chat)
        {
            posts.track(*tag, workspace);
        }
        order
    }

    pub(in crate::app::messenger) fn post_made(&mut self, via: Via, tag: u64, thread: i64) {
        let (rest, pointer) = self.messenger_mut(via).forum.posts.posted(tag, thread);
        let on = self.forum_on(via).is_some();
        if on {
            for order in rest {
                self.post(via, order);
            }
            if let Some((workspace, pointer)) = pointer {
                self.point_to_post(via, workspace, pointer);
            }
        } else {
            self.to_the_dm(via, rest);
        }
        self.refresh_messenger_note(via);
    }

    pub(in crate::app::messenger) fn post_failed(&mut self, via: Via, tag: u64) {
        let posts = &mut self.messenger_mut(via).forum.posts;
        let was = posts.failed;
        let orders = posts.failed(tag);
        let now = self.messenger(via).forum.posts.failed;
        self.to_the_dm(via, orders);
        if was != now {
            self.refresh_messenger_note(via);
        }
    }

    pub(in crate::app::messenger) fn post_gone(&mut self, via: Via, thread: i64, order: Order) {
        let forum = &mut self.messenger_mut(via).forum;
        if let Some(workspace) = forum.posts.workspace_of(thread) {
            forum.labels.unshown(workspace);
        }
        match forum.posts.gone(thread, order.tag()) {
            Some(true) => self.post_again(via, thread, order),
            Some(false) => self.to_the_dm(via, vec![order]),
            None => {}
        }
    }

    fn post_again(&mut self, via: Via, thread: i64, order: Order) {
        let route = order.tag().and_then(|tag| self.messenger(via).tags.get(&tag).copied());
        match self.routed(via, route, order) {
            Some(order) if matches!(order, Order::Send { chat, .. } if chat == thread) => {
                self.to_the_dm(via, vec![order]);
            }
            Some(order) => self.post(via, order),
            None => {}
        }
    }

    pub(in crate::app::messenger) fn post_archived(&mut self, via: Via, thread: i64) {
        let forum = &mut self.messenger_mut(via).forum;
        let Some(workspace) = forum.posts.workspace_of(thread) else { return };
        forum.posts.archive(workspace, true);
        forum.labels.unshown(workspace);
    }

    pub(in crate::app::messenger) fn posts_relinked(&mut self, via: Via) {
        let orders = self.messenger_mut(via).forum.posts.unmade();
        self.to_the_dm(via, orders);
    }

    fn to_the_dm(&self, via: Via, orders: Vec<Order>) {
        let Some(chat) = self.messenger(via).chat().map(|chat| chat.id) else { return };
        for order in orders {
            self.post(via, to_chat(order, chat));
        }
    }

    pub(in crate::app) fn forum_switched(&mut self) {
        if !self.config.discord_forum && self.showing_forum(Via::Discord) {
            self.overlay = None;
            self.close_forum_setup(Via::Discord);
        }
        let messenger = self.messenger(Via::Discord);
        if messenger.link.is_some() && messenger.forum_ready() {
            self.restart_link(Via::Discord);
        }
        self.refresh_messenger_note(Via::Discord);
    }
}
