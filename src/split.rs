use ratatui::layout::{Position, Rect};
use serde::{Deserialize, Serialize};

pub const HALF: f32 = 0.5;
pub const MIN_COLS: u16 = 10;
pub const MIN_ROWS: u16 = 3;
const PADDING: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    Right,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Swap,
    Left,
    Right,
    Above,
    Below,
}

impl Place {
    pub fn at(pane: Rect, pos: Position) -> Self {
        let (w, h) = (u32::from(pane.width.max(1)), u32::from(pane.height.max(1)));
        let x = u32::from(pos.x.saturating_sub(pane.x)) * 2 + 1;
        let y = u32::from(pos.y.saturating_sub(pane.y)) * 2 + 1;
        if (2 * w..4 * w).contains(&(3 * x)) && (2 * h..4 * h).contains(&(3 * y)) {
            return Self::Swap;
        }
        let edges = [
            (Self::Left, x * h),
            (Self::Right, (2 * w).saturating_sub(x) * h),
            (Self::Above, y * w),
            (Self::Below, (2 * h).saturating_sub(y) * w),
        ];
        edges.into_iter().min_by_key(|(_, d)| *d).map_or(Self::Swap, |(place, _)| place)
    }

    pub fn area(self, pane: Rect) -> Rect {
        let (w, h) = (pane.width / 2, pane.height / 2);
        match self {
            Self::Swap => pane,
            Self::Left => Rect { width: w, ..pane },
            Self::Right => Rect { x: pane.right() - w, width: w, ..pane },
            Self::Above => Rect { height: h, ..pane },
            Self::Below => Rect { y: pane.bottom() - h, height: h, ..pane },
        }
    }

    fn edge(self) -> Option<(Dir, bool)> {
        match self {
            Self::Swap => None,
            Self::Left => Some((Dir::Right, true)),
            Self::Right => Some((Dir::Right, false)),
            Self::Above => Some((Dir::Down, true)),
            Self::Below => Some((Dir::Down, false)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node<T> {
    Leaf(T),
    Split { dir: Dir, ratio: f32, first: Box<Node<T>>, second: Box<Node<T>> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    pub path: Vec<bool>,
    pub dir: Dir,
    pub area: Rect,
    pub line: Rect,
    min: (u16, u16),
}

fn gap(dir: Dir) -> u16 {
    match dir {
        Dir::Right => 1 + PADDING,
        Dir::Down => 1,
    }
}

fn leaf_min(dir: Dir) -> u16 {
    match dir {
        Dir::Right => MIN_COLS,
        Dir::Down => MIN_ROWS,
    }
}

fn span(area: Rect, dir: Dir) -> (u16, u16) {
    let total = match dir {
        Dir::Right => area.width,
        Dir::Down => area.height,
    };
    (total, gap(dir).min(total))
}

impl Divider {
    pub fn grab(&self) -> Rect {
        match self.dir {
            Dir::Right => Rect { width: self.line.width + PADDING, ..self.line }.intersection(self.area),
            Dir::Down => self.line,
        }
    }
}

fn divide(area: Rect, dir: Dir, ratio: f32, min: (u16, u16)) -> (Rect, Rect, Rect) {
    let (total, gap) = span(area, dir);
    let room = total - gap;
    let first = first_size(room, ratio, min);
    let second = room - first;
    match dir {
        Dir::Right => (
            Rect { width: first, ..area },
            Rect { x: area.x + first + gap, width: second, ..area },
            Rect { x: area.x + first, width: gap.min(1), ..area },
        ),
        Dir::Down => (
            Rect { height: first, ..area },
            Rect { y: area.y + first + gap, height: second, ..area },
            Rect { y: area.y + first, height: gap, ..area },
        ),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the ratio is clamped to 0..=1, so the product fits in the u16 it came from"
)]
fn first_size(room: u16, ratio: f32, min: (u16, u16)) -> u16 {
    if room < 2 {
        return room;
    }
    let (low, high) = bounds(room, min);
    let size = (f32::from(room) * ratio.clamp(0.0, 1.0)).round() as u16;
    size.clamp(low, high)
}

fn bounds(room: u16, (first, second): (u16, u16)) -> (u16, u16) {
    if first.saturating_add(second) <= room { (first, room - second) } else { (1, room - 1) }
}

pub fn ratio_at(divider: &Divider, pos: Position) -> f32 {
    let (start, at) = match divider.dir {
        Dir::Right => (divider.area.x, pos.x),
        Dir::Down => (divider.area.y, pos.y),
    };
    let (total, gap) = span(divider.area, divider.dir);
    let room = total - gap;
    if room < 2 {
        return HALF;
    }
    let (low, high) = bounds(room, divider.min);
    let first = at.saturating_sub(start).clamp(low, high);
    f32::from(first) / f32::from(room)
}

pub fn fits(area: Rect, dir: Dir) -> bool {
    let (total, gap) = span(area, dir);
    total - gap >= leaf_min(dir) * 2
}

impl<T: Copy + PartialEq> Node<T> {
    pub fn ids(&self) -> Vec<T> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    pub fn first(&self) -> T {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, .. } => first.first(),
        }
    }

    fn collect_ids(&self, ids: &mut Vec<T>) {
        match self {
            Self::Leaf(id) => ids.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    pub fn panes(&self, area: Rect) -> Vec<(T, Rect)> {
        let mut panes = Vec::new();
        self.collect_panes(area, &mut panes);
        panes
    }

    fn collect_panes(&self, area: Rect, panes: &mut Vec<(T, Rect)>) {
        match self {
            Self::Leaf(id) => panes.push((*id, area)),
            Self::Split { dir, ratio, first, second } => {
                let (a, b, _) = divide(area, *dir, *ratio, Self::sides(*dir, first, second));
                first.collect_panes(a, panes);
                second.collect_panes(b, panes);
            }
        }
    }

    fn sides(dir: Dir, first: &Self, second: &Self) -> (u16, u16) {
        (first.min_size(dir), second.min_size(dir))
    }

    fn min_size(&self, along: Dir) -> u16 {
        match self {
            Self::Leaf(_) => leaf_min(along),
            Self::Split { dir, first, second, .. } if *dir == along => {
                first.min_size(along).saturating_add(gap(along)).saturating_add(second.min_size(along))
            }
            Self::Split { first, second, .. } => first.min_size(along).max(second.min_size(along)),
        }
    }

    pub fn has_room(&self, area: Rect) -> bool {
        matches!(self, Self::Leaf(_))
            || (area.width >= self.min_size(Dir::Right) && area.height >= self.min_size(Dir::Down))
    }

    pub fn visible(&self, area: Rect, active: T) -> Vec<(T, Rect)> {
        if self.has_room(area) { self.panes(area) } else { vec![(active, area)] }
    }

    pub fn pane(&self, area: Rect, active: T, id: T) -> Option<Rect> {
        self.visible(area, active).into_iter().find(|(p, _)| *p == id).map(|(_, r)| r)
    }

    pub fn pane_at(&self, area: Rect, active: T, pos: Position) -> Option<T> {
        self.visible(area, active).into_iter().find(|(_, r)| r.contains(pos)).map(|(id, _)| id)
    }

    pub fn dividers(&self, area: Rect) -> Vec<Divider> {
        let mut dividers = Vec::new();
        if self.has_room(area) {
            self.collect_dividers(area, &mut Vec::new(), &mut dividers);
        }
        dividers
    }

    fn collect_dividers(&self, area: Rect, path: &mut Vec<bool>, dividers: &mut Vec<Divider>) {
        let Self::Split { dir, ratio, first, second } = self else { return };
        let min = Self::sides(*dir, first, second);
        let (a, b, line) = divide(area, *dir, *ratio, min);
        dividers.push(Divider { path: path.clone(), dir: *dir, area, line, min });
        path.push(false);
        first.collect_dividers(a, path, dividers);
        path.pop();
        path.push(true);
        second.collect_dividers(b, path, dividers);
        path.pop();
    }

    pub fn divider_at(&self, area: Rect, pos: Position) -> Option<Divider> {
        self.dividers(area).into_iter().find(|d| d.grab().contains(pos))
    }

    pub fn split(&mut self, target: T, dir: Dir, new: T) -> bool {
        self.insert(target, dir, false, new)
    }

    fn insert(&mut self, target: T, dir: Dir, before: bool, new: T) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                let (first, second) = if before { (new, target) } else { (target, new) };
                *self = Self::Split {
                    dir,
                    ratio: HALF,
                    first: Box::new(Self::Leaf(first)),
                    second: Box::new(Self::Leaf(second)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.insert(target, dir, before, new) || second.insert(target, dir, before, new)
            }
        }
    }

    pub fn moved(&self, pane: T, target: T, place: Place) -> Option<Self> {
        let ids = self.ids();
        if pane == target || !ids.contains(&pane) || !ids.contains(&target) {
            return None;
        }
        let mut node = self.clone();
        match place.edge() {
            None => node.swap(pane, target),
            Some((dir, before)) => {
                node.remove(pane)?;
                node.insert(target, dir, before, pane).then_some(())?;
            }
        }
        Some(node)
    }

    fn swap(&mut self, a: T, b: T) {
        match self {
            Self::Leaf(id) if *id == a => *id = b,
            Self::Leaf(id) if *id == b => *id = a,
            Self::Leaf(_) => {}
            Self::Split { first, second, .. } => {
                first.swap(a, b);
                second.swap(a, b);
            }
        }
    }

    pub fn remove(&mut self, target: T) -> Option<T> {
        let Self::Split { first, second, .. } = self else { return None };
        let (kept, neighbour) = if **first == Self::Leaf(target) {
            (std::mem::replace(second.as_mut(), Self::Leaf(target)), true)
        } else if **second == Self::Leaf(target) {
            (std::mem::replace(first.as_mut(), Self::Leaf(target)), false)
        } else {
            return first.remove(target).or_else(|| second.remove(target));
        };
        let ids = kept.ids();
        let near = if neighbour { ids.first() } else { ids.last() }.copied();
        *self = kept;
        near
    }

    pub fn set_ratio(&mut self, path: &[bool], value: f32) -> bool {
        match (self, path.split_first()) {
            (Self::Split { ratio, .. }, None) => {
                *ratio = value;
                true
            }
            (Self::Split { first, second, .. }, Some((&go_second, rest))) => {
                if go_second {
                    second.set_ratio(rest, value)
                } else {
                    first.set_ratio(rest, value)
                }
            }
            (Self::Leaf(_), _) => false,
        }
    }

    pub fn map<U>(&self, f: &impl Fn(T) -> Option<U>) -> Option<Node<U>> {
        Some(match self {
            Self::Leaf(id) => Node::Leaf(f(*id)?),
            Self::Split { dir, ratio, first, second } => Node::Split {
                dir: *dir,
                ratio: *ratio,
                first: Box::new(first.map(f)?),
                second: Box::new(second.map(f)?),
            },
        })
    }
}

pub fn row<T: Copy>(ids: &[T]) -> Option<Node<T>> {
    let (last, rest) = ids.split_last()?;
    let mut node = Node::Leaf(*last);
    for (i, id) in rest.iter().enumerate().rev() {
        #[expect(clippy::cast_precision_loss, reason = "a tab never holds millions of panes")]
        let ratio = 1.0 / (ids.len() - i) as f32;
        node = Node::Split { dir: Dir::Right, ratio, first: Box::new(Node::Leaf(*id)), second: Box::new(node) };
    }
    Some(node)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const AREA: Rect = Rect { x: 10, y: 2, width: 42, height: 21 };

    fn pair(dir: Dir) -> Node<u64> {
        let mut node = Node::Leaf(1);
        node.split(1, dir, 2);
        node
    }

    fn three() -> Node<u64> {
        let mut node = pair(Dir::Right);
        node.split(2, Dir::Down, 3);
        node
    }

    mod geometry {
        use super::*;

        #[test]
        fn a_leaf_takes_the_whole_area() {
            assert_eq!(Node::Leaf(7).panes(AREA), vec![(7, AREA)]);
        }

        #[test]
        fn split_right_leaves_the_divider_and_a_blank_column_between_the_halves() {
            let panes = pair(Dir::Right).panes(AREA);
            assert_eq!(panes, vec![(1, Rect::new(10, 2, 20, 21)), (2, Rect::new(32, 2, 20, 21))]);
        }

        #[test]
        fn split_down_leaves_one_row_between_the_halves() {
            let panes = pair(Dir::Down).panes(AREA);
            assert_eq!(panes, vec![(1, Rect::new(10, 2, 42, 10)), (2, Rect::new(10, 13, 42, 10))]);
        }

        #[test]
        fn the_divider_sits_between_the_halves() {
            let dividers = pair(Dir::Right).dividers(AREA);
            assert_eq!(dividers.iter().map(|d| d.line).collect::<Vec<_>>(), vec![Rect::new(30, 2, 1, 21)]);
        }

        #[test]
        fn nested_dividers_carry_their_path() {
            let paths: Vec<Vec<bool>> = three().dividers(AREA).into_iter().map(|d| d.path).collect();
            assert_eq!(paths, vec![vec![], vec![true]]);
        }

        #[test]
        fn halves_never_shrink_below_their_minimum() {
            let mut node = pair(Dir::Right);
            node.set_ratio(&[], 0.0);
            assert_eq!(node.panes(AREA)[0].1.width, MIN_COLS);
        }

        #[test]
        fn a_pane_is_found_under_the_mouse() {
            assert_eq!(three().pane_at(AREA, 1, Position::new(40, 20)), Some(3));
        }

        #[test]
        fn the_divider_is_not_a_pane() {
            assert_eq!(pair(Dir::Right).pane_at(AREA, 1, Position::new(30, 5)), None);
        }

        #[test]
        fn the_blank_column_after_the_divider_also_grabs_it() {
            assert_eq!(pair(Dir::Right).divider_at(AREA, Position::new(31, 5)).map(|d| d.path), Some(vec![]));
        }
    }

    mod room {
        use super::*;

        const NARROW: Rect = Rect { width: 15, ..AREA };

        fn stacked() -> Node<u64> {
            let mut node = pair(Dir::Down);
            node.split(2, Dir::Down, 3);
            node
        }

        #[rstest]
        #[case::a_pane(Node::Leaf(1), (MIN_COLS, MIN_ROWS))]
        #[case::side_by_side(pair(Dir::Right), (2 * MIN_COLS + 2, MIN_ROWS))]
        #[case::one_above_the_other(pair(Dir::Down), (MIN_COLS, 2 * MIN_ROWS + 1))]
        #[case::nested(three(), (2 * MIN_COLS + 2, 2 * MIN_ROWS + 1))]
        fn a_tree_needs_room_for_every_pane(#[case] node: Node<u64>, #[case] expected: (u16, u16)) {
            assert_eq!((node.min_size(Dir::Right), node.min_size(Dir::Down)), expected);
        }

        #[test]
        fn dragging_the_outer_divider_to_the_edge_keeps_the_nested_panes() {
            let mut node = stacked();
            let outer = node.dividers(AREA).remove(0);
            node.set_ratio(&outer.path, ratio_at(&outer, Position::new(AREA.x, AREA.bottom() - 1)));
            assert!(node.panes(AREA).iter().all(|(_, r)| r.height >= MIN_ROWS), "{:?}", node.panes(AREA));
        }

        #[test]
        fn dragging_to_the_other_edge_keeps_the_first_pane() {
            let mut node = pair(Dir::Right);
            let divider = node.dividers(AREA).remove(0);
            node.set_ratio(&divider.path, ratio_at(&divider, Position::new(AREA.x, 5)));
            assert_eq!(node.panes(AREA)[0].1.width, MIN_COLS);
        }

        #[test]
        fn a_smaller_area_keeps_every_pane_while_the_tree_fits() {
            let mut node = stacked();
            node.set_ratio(&[], 0.9);
            let low = Rect { height: stacked().min_size(Dir::Down), ..AREA };
            assert!(node.panes(low).iter().all(|(_, r)| r.height >= MIN_ROWS), "{:?}", node.panes(low));
        }

        #[test]
        fn with_room_every_pane_shows() {
            assert_eq!(three().visible(AREA, 3), three().panes(AREA));
        }

        #[test]
        fn without_room_for_every_pane_only_the_active_one_shows() {
            assert_eq!(three().visible(NARROW, 3), vec![(3, NARROW)]);
        }

        #[test]
        fn a_single_pane_always_shows() {
            let tiny = Rect { width: 3, height: 1, ..AREA };
            assert_eq!(Node::Leaf(1).visible(tiny, 1), vec![(1, tiny)]);
        }

        #[test]
        fn without_room_there_are_no_dividers() {
            assert_eq!(three().dividers(NARROW), Vec::new());
        }

        #[test]
        fn without_room_the_hidden_panes_have_no_place() {
            let tree = three();
            let under = tree.pane_at(NARROW, 3, Position::new(NARROW.x, NARROW.y));
            assert_eq!((tree.pane(NARROW, 3, 1), tree.pane(NARROW, 3, 3), under), (None, Some(NARROW), Some(3)));
        }
    }

    mod editing {
        use super::*;

        #[test]
        fn removing_a_pane_gives_its_space_to_the_sibling() {
            let mut node = three();
            node.remove(3);
            assert_eq!(node, pair(Dir::Right));
        }

        #[test]
        fn removing_the_first_half_keeps_the_second() {
            let mut node = pair(Dir::Right);
            node.remove(1);
            assert_eq!(node, Node::Leaf(2));
        }

        #[rstest]
        #[case::the_first_goes_to_the_nearest_of_the_rest(1, Some(2))]
        #[case::the_last_goes_to_its_sibling(3, Some(2))]
        #[case::a_missing_one_changes_nothing(9, None)]
        fn removing_names_the_pane_that_takes_the_space(#[case] removed: u64, #[case] near: Option<u64>) {
            assert_eq!(three().remove(removed), near);
        }

        #[test]
        fn the_ratio_of_a_nested_split_changes_by_path() {
            let mut node = three();
            node.set_ratio(&[true], 0.25);
            let Node::Split { second, .. } = node else { panic!("expected a split") };
            assert!(matches!(*second, Node::Split { ratio, .. } if (ratio - 0.25).abs() < f32::EPSILON));
        }

        #[test]
        fn dragging_puts_the_divider_under_the_mouse() {
            let mut node = pair(Dir::Right);
            let divider = node.dividers(AREA).remove(0);
            node.set_ratio(&divider.path, ratio_at(&divider, Position::new(20, 5)));
            assert_eq!(node.dividers(AREA)[0].line.x, 20);
        }

        #[test]
        fn a_row_shares_the_width() {
            let widths: Vec<u16> = row(&[1, 2, 3]).expect("a row").panes(AREA).iter().map(|(_, r)| r.width).collect();
            assert_eq!(widths, vec![13, 13, 12]);
        }

        #[test]
        fn map_drops_the_tree_when_an_id_is_unknown() {
            assert_eq!(three().map(&|id| (id != 3).then_some(id)), None);
        }

        #[test]
        fn two_panes_side_by_side_can_be_stacked() {
            assert_eq!(pair(Dir::Right).moved(2, 1, Place::Below), Some(pair(Dir::Down)));
        }

        #[test]
        fn a_pane_dropped_above_its_sibling_goes_first() {
            let expected = Node::Split {
                dir: Dir::Down,
                ratio: HALF,
                first: Box::new(Node::Leaf(2)),
                second: Box::new(Node::Leaf(1)),
            };
            assert_eq!(pair(Dir::Right).moved(2, 1, Place::Above), Some(expected));
        }

        #[rstest]
        #[case::to_the_left(Place::Left, Dir::Right, [3, 1])]
        #[case::to_the_right(Place::Right, Dir::Right, [1, 3])]
        #[case::above(Place::Above, Dir::Down, [3, 1])]
        #[case::below(Place::Below, Dir::Down, [1, 3])]
        fn a_moved_pane_leaves_its_place_and_splits_the_target(
            #[case] place: Place,
            #[case] dir: Dir,
            #[case] order: [u64; 2],
        ) {
            let [first, second] = order;
            let mut target = Node::Leaf(first);
            target.split(first, dir, second);
            let expected =
                Node::Split { dir: Dir::Right, ratio: HALF, first: Box::new(target), second: Box::new(Node::Leaf(2)) };
            assert_eq!(three().moved(3, 1, place), Some(expected));
        }

        #[test]
        fn swapping_keeps_the_layout() {
            let swapped = three().moved(1, 3, Place::Swap).expect("a new layout");
            let mut expected = Node::Leaf(3);
            expected.split(3, Dir::Right, 2);
            expected.split(2, Dir::Down, 1);
            assert_eq!(swapped, expected);
        }

        #[rstest]
        #[case::onto_itself(1, 1)]
        #[case::an_unknown_pane(9, 1)]
        #[case::onto_an_unknown_target(1, 9)]
        fn a_move_that_cannot_happen_gives_nothing(#[case] pane: u64, #[case] target: u64) {
            assert_eq!(three().moved(pane, target, Place::Left), None);
        }

        #[rstest]
        #[case::the_middle(Position::new(20, 10), Place::Swap)]
        #[case::near_the_left_edge(Position::new(1, 10), Place::Left)]
        #[case::near_the_right_edge(Position::new(39, 10), Place::Right)]
        #[case::near_the_top(Position::new(20, 1), Place::Above)]
        #[case::near_the_bottom(Position::new(20, 19), Place::Below)]
        #[case::a_corner_goes_to_the_nearer_edge(Position::new(2, 0), Place::Above)]
        fn where_a_pane_lands_follows_the_pointer(#[case] pos: Position, #[case] expected: Place) {
            assert_eq!(Place::at(Rect::new(0, 0, 40, 20), pos), expected);
        }

        #[rstest]
        #[case::swap(Place::Swap, Rect::new(10, 2, 41, 21))]
        #[case::left(Place::Left, Rect::new(10, 2, 20, 21))]
        #[case::right(Place::Right, Rect::new(31, 2, 20, 21))]
        #[case::above(Place::Above, Rect::new(10, 2, 41, 10))]
        #[case::below(Place::Below, Rect::new(10, 13, 41, 10))]
        fn the_landing_covers_the_half_the_pane_takes(#[case] place: Place, #[case] expected: Rect) {
            assert_eq!(place.area(Rect::new(10, 2, 41, 21)), expected);
        }

        #[rstest]
        #[case::wide_enough_for_right(Rect::new(0, 0, 22, 5), Dir::Right, true)]
        #[case::too_narrow_for_right(Rect::new(0, 0, 21, 50), Dir::Right, false)]
        #[case::tall_enough_for_down(Rect::new(0, 0, 5, 7), Dir::Down, true)]
        #[case::too_short_for_down(Rect::new(0, 0, 80, 6), Dir::Down, false)]
        fn a_split_needs_room_for_both_halves(#[case] area: Rect, #[case] dir: Dir, #[case] expected: bool) {
            assert_eq!(fits(area, dir), expected);
        }
    }
}
