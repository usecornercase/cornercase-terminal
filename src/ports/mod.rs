use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::PathBuf;

use crate::host::open::Browse;
use crate::process;
use crate::process::owners::{Owner, Trees, folder, owner, trees};

pub(crate) mod dialog;
pub(crate) mod listen;
pub(crate) mod stop;

use listen::Listener;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pane {
    pub(crate) id: u64,
    pub(crate) shell: i32,
    pub(crate) workspace: u64,
    pub(crate) agent: Option<(String, i32)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) panes: Vec<Pane>,
    pub(crate) worktrees: Vec<(PathBuf, u64)>,
    pub(crate) server: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    Pane,
    Agent,
    Cwd,
}

impl Via {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Pane => "pane",
            Self::Agent => "agent",
            Self::Cwd => "cwd",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) workspace: Option<u64>,
    pub(crate) pane: Option<u64>,
    pub(crate) via: Option<Via>,
    pub(crate) agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Port {
    pub(crate) number: u16,
    pub(crate) addresses: Vec<IpAddr>,
    pub(crate) pid: i32,
    pub(crate) program: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) started: Option<u64>,
    pub(crate) place: Place,
}

impl Port {
    pub(crate) fn browse(&self) -> Browse {
        Browse { ip: host(&self.addresses), port: self.number }
    }

    pub(crate) fn url(&self) -> String {
        self.browse().url()
    }
}

pub(crate) fn scan(places: &Places, only: Option<u64>) -> Vec<Port> {
    let pids: Vec<i32> = process::all().into_iter().filter(|pid| *pid != places.server).collect();
    let attribution = Attribution::new(places);
    let mut ports = merge(listen::listeners(&pids))
        .into_iter()
        .map(|((pid, number), addresses)| Port {
            number,
            addresses,
            pid,
            program: process::name(pid),
            args: process::args(pid),
            started: process::start_stamp(pid),
            place: attribution.place(pid),
        })
        .filter(|port| only.is_none() || port.place.workspace == only)
        .collect::<Vec<_>>();
    ports.sort_by_key(|port| (port.number, port.pid));
    ports
}

pub(crate) fn merge(listeners: Vec<Listener>) -> BTreeMap<(i32, u16), Vec<IpAddr>> {
    let mut merged: BTreeMap<(i32, u16), Vec<IpAddr>> = BTreeMap::new();
    for Listener { pid, port, address } in listeners {
        let addresses = merged.entry((pid, port)).or_default();
        if !addresses.contains(&address) {
            addresses.push(address);
            addresses.sort_unstable();
        }
    }
    merged
}

pub(crate) fn host(addresses: &[IpAddr]) -> Option<IpAddr> {
    let local = |a: &IpAddr| a.is_unspecified() || (a.is_ipv4() && a.is_loopback());
    if addresses.iter().any(local) {
        return None;
    }
    let chosen = addresses.iter().find(|a| a.is_ipv4()).or_else(|| addresses.iter().find(|a| !a.is_loopback()));
    chosen.or_else(|| addresses.first()).copied()
}

pub(crate) struct Attribution<'a> {
    places: &'a Places,
    panes: Trees<usize>,
    agents: Trees<usize>,
    worktrees: Vec<PathBuf>,
}

impl<'a> Attribution<'a> {
    pub(crate) fn new(places: &'a Places) -> Self {
        let shells: Vec<(i32, usize)> = places.panes.iter().enumerate().map(|(i, pane)| (pane.shell, i)).collect();
        let agents: Vec<(i32, usize)> =
            places.panes.iter().enumerate().filter_map(|(i, pane)| Some((pane.agent.as_ref()?.1, i))).collect();
        Self {
            places,
            panes: trees(&shells),
            agents: trees(&agents),
            worktrees: places
                .worktrees
                .iter()
                .map(|(path, _)| path.canonicalize().unwrap_or_else(|_| path.clone()))
                .collect(),
        }
    }

    pub(crate) fn place(&self, pid: i32) -> Place {
        if let Owner::Pane(&at) = owner(pid, &self.panes, self.places.server) {
            let pane = &self.places.panes[at];
            let agent = matches!(owner(pid, &self.agents, self.places.server), Owner::Pane(_));
            return Place {
                workspace: Some(pane.workspace),
                pane: Some(pane.id),
                via: Some(if agent { Via::Agent } else { Via::Pane }),
                agent: pane.agent.as_ref().filter(|_| agent).map(|(name, _)| name.clone()),
            };
        }
        let at = process::cwd(pid).and_then(|cwd| folder(&cwd, &self.worktrees));
        at.map_or_else(Place::default, |at| Place {
            workspace: Some(self.places.worktrees[at].1),
            via: Some(Via::Cwd),
            ..Place::default()
        })
    }
}

#[cfg(test)]
mod tests;
