use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

use super::{Pane, PaneTree, Places, Proc, Sample, Times, cpu_percent};
use crate::process;

type Edges = Vec<(i32, i32)>;

struct Walk {
    panes: Vec<(Pane, Edges)>,
    cornercase: Edges,
}

pub(crate) fn sample(places: &Places, previous: Option<Times>, baseline: Duration) -> Sample {
    let before = previous.unwrap_or_else(|| {
        let first = read(places, &Times::default());
        thread::sleep(baseline);
        first.times
    });
    read(places, &before)
}

fn walk(places: &Places) -> Walk {
    let shells: Vec<i32> = places.panes.iter().map(|pane| pane.shell).collect();
    let panes = places.panes.iter().map(|pane| (pane.clone(), process::tree(pane.shell))).collect();
    let own: Edges = process::children(places.server)
        .into_iter()
        .filter(|_| places.own)
        .filter(|child| !shells.contains(child))
        .flat_map(|child| std::iter::once((child, places.server)).chain(process::tree(child)).collect::<Vec<_>>())
        .collect();
    Walk { panes, cornercase: own }
}

fn read(places: &Places, before: &Times) -> Sample {
    let walk = walk(places);
    let now = Instant::now();
    let mut times = Times { at: Some(now), cpu: HashMap::new() };
    let panes = walk
        .panes
        .into_iter()
        .map(|(pane, edges)| {
            let procs = tree_of(pane.shell, &edges, before, &mut times);
            PaneTree { pane, procs }
        })
        .collect();
    let cornercase = if places.own { tree_of(places.server, &walk.cornercase, before, &mut times) } else { Vec::new() };
    Sample { panes, cornercase, times }
}

fn tree_of(root: i32, edges: &Edges, before: &Times, times: &mut Times) -> Vec<Proc> {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for (child, parent) in edges {
        children.entry(*parent).or_default().push(*child);
    }
    let mut procs = Vec::new();
    let mut stack = vec![(root, None::<i32>, 0_usize)];
    while let Some((pid, parent, depth)) = stack.pop() {
        if let Some(proc) = proc_of(pid, parent, depth, before, times) {
            procs.push(proc);
        }
        for child in children.get(&pid).into_iter().flatten().rev() {
            stack.push((*child, Some(pid), depth + 1));
        }
    }
    procs
}

fn proc_of(pid: i32, parent: Option<i32>, depth: usize, before: &Times, times: &mut Times) -> Option<Proc> {
    if !process::running(pid, None) {
        return None;
    }
    let started = process::start_stamp(pid);
    let cpu = process::cpu_time(pid);
    if let Some(cpu) = cpu {
        times.cpu.insert((pid, started), cpu);
    }
    let name = process::name(pid)?;
    let percent = cpu.and_then(|_| cpu_percent(before, times, (pid, started)));
    Some(Proc {
        pid,
        parent,
        depth,
        name,
        args: process::args(pid),
        started,
        cpu: percent,
        memory: process::footprint(pid),
    })
}
