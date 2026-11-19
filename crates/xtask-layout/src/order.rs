//! Role: 層ごとの並びと port の順を決め、交差を数える（付属書 L の 5 章）。
use crate::level::{BoxItem, Chains, ItemId, ItemKind, Link, Order};
use crate::model::{LayoutError, MAX_SWEEPS, TURN_HEIGHT};

/// 転置の往復の上限（5.5 節）。
const TRANSPOSE_PASSES: usize = 4;

/// 交差の数え方で使う、層の中の位置と port の順の組。
type Key = (usize, usize);

/// port が item の左辺か右辺のどちらに付くか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

/// item ごとの、隣の層へ向かう link（`out_links`：item が left）と、
/// 左の層から来る link（`in_links`：item が right）。index の昇順。
struct Adjacency {
    out_links: Vec<Vec<usize>>,
    in_links: Vec<Vec<usize>>,
}

impl Adjacency {
    fn new(chains: &Chains) -> Self {
        let mut out_links = vec![Vec::new(); chains.items.len()];
        let mut in_links = vec![Vec::new(); chains.items.len()];
        for (index, link) in chains.links.iter().enumerate() {
            if let Some(list) = out_links.get_mut(link.left) {
                list.push(index);
            }
            if let Some(list) = in_links.get_mut(link.right) {
                list.push(index);
            }
        }
        Self {
            out_links,
            in_links,
        }
    }
}

/// 層ごとの並びと、port の距離を決める（5.1 節〜5.7 節）。
pub(crate) fn arrange(chains: &Chains) -> Result<Order, LayoutError> {
    let adjacency = Adjacency::new(chains);
    let layers = sweep(chains, &adjacency, initial_layers(chains));
    let pos = positions(chains.items.len(), &layers);
    let (east, west) = assign_ports(chains, &adjacency, &pos)?;
    Ok(Order { layers, east, west })
}

/// 層 (L, L+1) ごとの交差の数（5.6 節）。
pub(crate) fn crossings(chains: &Chains, layers: &[Vec<ItemId>]) -> usize {
    let pos = positions(chains.items.len(), layers);
    let mut groups: Vec<Vec<(Key, Key)>> = vec![Vec::new(); layers.len()];
    for link in &chains.links {
        let Some(&layer) = chains.layer.get(link.left) else {
            continue;
        };
        let left = (
            pos[link.left],
            port_rank(chains, link.left, Side::Right, link.edge),
        );
        let right = (
            pos[link.right],
            port_rank(chains, link.right, Side::Left, link.edge),
        );
        if let Some(group) = groups.get_mut(layer) {
            group.push((left, right));
        }
    }
    groups
        .iter_mut()
        .map(|group| {
            group.sort();
            let mut rights: Vec<Key> = group.iter().map(|&(_, right)| right).collect();
            inversions(&mut rights)
        })
        .sum()
}

/// 各層の item を index の昇順に並べた初めの並び（5.1 節）。
fn initial_layers(chains: &Chains) -> Vec<Vec<ItemId>> {
    let mut layers = vec![Vec::new(); chains.layer_count];
    for (item, &layer) in chains.layer.iter().enumerate() {
        if let Some(members) = layers.get_mut(layer) {
            members.push(item);
        }
    }
    layers
}

/// item の、層の中の位置（0 から）。
fn positions(item_count: usize, layers: &[Vec<ItemId>]) -> Vec<usize> {
    let mut pos = vec![0; item_count];
    for layer in layers {
        set_positions(&mut pos, layer);
    }
    pos
}

fn set_positions(pos: &mut [usize], layer: &[ItemId]) {
    for (rank, &item) in layer.iter().enumerate() {
        if let Some(slot) = pos.get_mut(item) {
            *slot = rank;
        }
    }
}

/// 交差の削減（5.3 節）。改善しなかった往復の後も、次の往復は今の並びから続ける。
fn sweep(chains: &Chains, adjacency: &Adjacency, initial: Vec<Vec<ItemId>>) -> Vec<Vec<ItemId>> {
    let mut best_count = crossings(chains, &initial);
    let mut best = initial.clone();
    let mut current = initial;
    let mut stall = 0;
    for _ in 0..MAX_SWEEPS {
        let mut pos = positions(chains.items.len(), &current);
        for layer in current.iter_mut().skip(1) {
            reorder(chains, adjacency, &mut pos, layer, true);
        }
        for layer in (0..current.len().saturating_sub(1)).rev() {
            reorder(chains, adjacency, &mut pos, &mut current[layer], false);
        }
        transpose(chains, adjacency, &mut pos, &mut current);
        let count = crossings(chains, &current);
        if count < best_count {
            best_count = count;
            best = current.clone();
            stall = 0;
        } else {
            stall += 1;
            if stall == 2 {
                break;
            }
        }
        if best_count == 0 {
            break;
        }
    }
    best
}

/// 一つの層を、固定した隣の層の重心の順に並べ直す（5.4 節）。
/// `down` のときは左の層（L-1）を、そうでなければ右の層（L+1）を固定する。
fn reorder(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &mut [usize],
    layer: &mut [ItemId],
    down: bool,
) {
    let fixed: &[usize] = pos;
    let mut keyed: Vec<(f64, usize, ItemId)> = layer
        .iter()
        .enumerate()
        .map(|(rank, &item)| {
            let value = barycenter(chains, adjacency, fixed, item, down).unwrap_or(rank as f64);
            (value, rank, item)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (slot, (_, _, item)) in layer.iter_mut().zip(keyed) {
        *slot = item;
    }
    set_positions(pos, layer);
}

/// 固定の隣の層にある port の位置の値の平均。隣が無ければ None。
fn barycenter(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &[usize],
    item: ItemId,
    down: bool,
) -> Option<f64> {
    let links = if down {
        &adjacency.in_links[item]
    } else {
        &adjacency.out_links[item]
    };
    if links.is_empty() {
        return None;
    }
    let sum: f64 = links
        .iter()
        .map(|&index| {
            let link = &chains.links[index];
            if down {
                left_end_value(chains, pos, link)
            } else {
                right_end_value(chains, pos, link)
            }
        })
        .sum();
    Some(sum / links.len() as f64)
}

/// 隣り合う二つの item を入れ替えて交差が減るなら入れ替える（5.5 節）。
fn transpose(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &mut [usize],
    layers: &mut [Vec<ItemId>],
) {
    for _ in 0..TRANSPOSE_PASSES {
        let mut changed = false;
        for layer in layers.iter_mut() {
            for i in 0..layer.len().saturating_sub(1) {
                let upper = layer[i];
                let lower = layer[i + 1];
                let keep = pair_cost(chains, adjacency, pos, upper, lower);
                let swap = pair_cost(chains, adjacency, pos, lower, upper);
                if swap < keep {
                    layer.swap(i, i + 1);
                    pos[upper] = i + 1;
                    pos[lower] = i;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// `upper` を `lower` の上に置いたときの、両隣の層との交差の数。
fn pair_cost(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &[usize],
    upper: ItemId,
    lower: ItemId,
) -> usize {
    let u = neighbour_values(chains, adjacency, pos, upper);
    let v = neighbour_values(chains, adjacency, pos, lower);
    crossing_pairs(&u[0], &v[0]) + crossing_pairs(&u[1], &v[1])
}

/// item の、左の層（[0]）と右の層（[1]）に向かう link の相手の port の位置の値。
fn neighbour_values(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &[usize],
    item: ItemId,
) -> [Vec<f64>; 2] {
    let left = adjacency.in_links[item]
        .iter()
        .map(|&index| left_end_value(chains, pos, &chains.links[index]))
        .collect();
    let right = adjacency.out_links[item]
        .iter()
        .map(|&index| right_end_value(chains, pos, &chains.links[index]))
        .collect();
    [left, right]
}

/// upper の値が lower の値より大きい組の数。
fn crossing_pairs(upper: &[f64], lower: &[f64]) -> usize {
    upper
        .iter()
        .map(|&a| lower.iter().filter(|&&b| a > b).count())
        .sum()
}

/// 併合整列で、逆順の組（i < j かつ a[i] > a[j]）の数を数え、a を整列する。
fn inversions<T: Copy + Ord>(values: &mut [T]) -> usize {
    let len = values.len();
    if len < 2 {
        return 0;
    }
    let (left, right) = values.split_at_mut(len / 2);
    let mut count = inversions(left) + inversions(right);
    let mut merged = Vec::with_capacity(len);
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] {
            merged.push(left[i]);
            i += 1;
        } else {
            merged.push(right[j]);
            j += 1;
            count += left.len() - i;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    values.copy_from_slice(&merged);
    count
}

/// Box の同じ辺の port を y の昇順に並べたときの順位。Box 以外は 0。
fn port_rank(chains: &Chains, item: ItemId, side: Side, edge: usize) -> usize {
    let ItemKind::Box(port) = &chains.items[item].kind else {
        return 0;
    };
    let list = side_ports(port, side);
    let Some(own) = list
        .iter()
        .find(|entry| entry.0 == edge)
        .map(|&(e, y)| (y, e))
    else {
        return 0;
    };
    list.iter().filter(|&&(e, y)| (y, e) < own).count()
}

fn side_ports(port: &BoxItem, side: Side) -> &[(usize, i64)] {
    match side {
        Side::Left => port.entries.as_slice(),
        Side::Right => port.exits.as_slice(),
    }
}

fn box_port(port: &BoxItem, side: Side, edge: usize) -> Option<i64> {
    side_ports(port, side)
        .iter()
        .find(|entry| entry.0 == edge)
        .map(|&(_, y)| y)
}

/// item の port の位置の値（5.2 節）。Box は、その辺の port の y で小数の部分を決める。
fn port_value(chains: &Chains, pos: &[usize], item: ItemId, side: Side, edge: usize) -> f64 {
    let frac = match &chains.items[item].kind {
        ItemKind::Box(port) => box_port(port, side, edge).map_or(0.5, |y| {
            y as f64 / (chains.items[item].size.height + 1) as f64
        }),
        _ => 0.5,
    };
    pos[item] as f64 + frac
}

/// link の左の item（右辺の port）の位置の値。
fn left_end_value(chains: &Chains, pos: &[usize], link: &Link) -> f64 {
    port_value(chains, pos, link.left, Side::Right, link.edge)
}

/// link の右の item（左辺の port）の位置の値。
fn right_end_value(chains: &Chains, pos: &[usize], link: &Link) -> f64 {
    port_value(chains, pos, link.right, Side::Left, link.edge)
}

/// link ごとに、その link が最初に現れる path の番号と step の番号。
fn path_order(chains: &Chains) -> Vec<Option<(usize, usize)>> {
    let mut order = vec![None; chains.links.len()];
    for (path_index, path) in chains.paths.iter().enumerate() {
        for (step_index, step) in path.iter().enumerate() {
            if let Some(slot) = order.get_mut(step.link).filter(|slot| slot.is_none()) {
                *slot = Some((path_index, step_index));
            }
        }
    }
    order
}

/// 最良の並びで、link ごとの east（左の item の右辺）と west（右の item の左辺）を決める（5.7 節）。
fn assign_ports(
    chains: &Chains,
    adjacency: &Adjacency,
    pos: &[usize],
) -> Result<(Vec<i64>, Vec<i64>), LayoutError> {
    let mut east: Vec<i64> = vec![0; chains.links.len()];
    let mut west: Vec<i64> = vec![0; chains.links.len()];
    let order = path_order(chains);
    for (item, entry) in chains.items.iter().enumerate() {
        let out = &adjacency.out_links[item];
        let inward = &adjacency.in_links[item];
        match &entry.kind {
            ItemKind::Box(port) => {
                box_ports(chains, port, out, Side::Right, &mut east)?;
                box_ports(chains, port, inward, Side::Left, &mut west)?;
            }
            ItemKind::TurnEast(_) => {
                turn_ports(chains, pos, &order, inward, |link| link.left, &mut west)?
            }
            ItemKind::TurnWest(_) => {
                turn_ports(chains, pos, &order, out, |link| link.right, &mut east)?
            }
            _ => {
                spread(
                    chains,
                    out,
                    |link| right_end_value(chains, pos, link),
                    entry.size.height,
                    &mut east,
                );
                spread(
                    chains,
                    inward,
                    |link| left_end_value(chains, pos, link),
                    entry.size.height,
                    &mut west,
                );
            }
        }
    }
    Ok((east, west))
}

/// Box の辺の port の y を、link の距離にする。
fn box_ports(
    chains: &Chains,
    port: &BoxItem,
    links: &[usize],
    side: Side,
    ports: &mut [i64],
) -> Result<(), LayoutError> {
    for &index in links {
        let edge = chains.links[index].edge;
        ports[index] = box_port(port, side, edge)
            .ok_or_else(|| LayoutError::Internal(format!("box port for edge {edge} is missing")))?;
    }
    Ok(())
}

/// U ターンの二つの port。相手の位置が小さいほうを 0、大きいほうを `TURN_HEIGHT` にする。
/// 同じ位置なら path で先に来る link を 0 にする。
fn turn_ports(
    chains: &Chains,
    pos: &[usize],
    order: &[Option<(usize, usize)>],
    links: &[usize],
    partner: fn(&Link) -> ItemId,
    ports: &mut [i64],
) -> Result<(), LayoutError> {
    let mut sorted = links.to_vec();
    sorted.sort_by_key(|&index| (pos[partner(&chains.links[index])], order[index]));
    match sorted.as_slice() {
        [first, second] => {
            ports[*first] = 0;
            ports[*second] = TURN_HEIGHT;
            Ok(())
        }
        _ => Err(LayoutError::Internal(format!(
            "a turn has {} links, expected 2",
            links.len()
        ))),
    }
}

/// 高さ `height` の辺に、link を相手の位置の値の順（同じなら edge の index の順）で等間隔に並べる。
fn spread(
    chains: &Chains,
    links: &[usize],
    value: impl Fn(&Link) -> f64,
    height: i64,
    ports: &mut [i64],
) {
    let mut keyed: Vec<(f64, usize, usize)> = links
        .iter()
        .map(|&index| (value(&chains.links[index]), chains.links[index].edge, index))
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let count = keyed.len() as i64;
    for (k, &(_, _, index)) in keyed.iter().enumerate() {
        ports[index] = ((k as i64 + 1) * height) / (count + 1);
    }
}

#[cfg(test)]
mod tests {
    //! Role: 層の中の並び順（交差の削減と port の順）の試験。
    use super::*;
    use crate::level::{Item, ItemKind, Link, Step};
    use crate::model::{Size, TURN_HEIGHT};

    fn node(index: usize) -> Item {
        Item {
            kind: ItemKind::Node(index),
            size: Size {
                width: 80,
                height: 40,
            },
        }
    }

    /// 層 0 に a, b、層 1 に c, d。link a→d、b→c で二本が交差する。
    fn crossed_chains() -> Chains {
        Chains {
            items: vec![node(0), node(1), node(2), node(3)],
            layer: vec![0, 0, 1, 1],
            layer_count: 2,
            links: vec![
                Link {
                    edge: 0,
                    left: 0,
                    right: 3,
                },
                Link {
                    edge: 1,
                    left: 1,
                    right: 2,
                },
            ],
            paths: vec![
                vec![Step {
                    link: 0,
                    forward: true,
                }],
                vec![Step {
                    link: 1,
                    forward: true,
                }],
            ],
        }
    }

    #[test]
    fn crossings_counts_inversions() {
        let chains = crossed_chains();
        let initial = vec![vec![0, 1], vec![2, 3]];
        assert_eq!(crossings(&chains, &initial), 1);
    }

    #[test]
    fn two_crossing_links_uncross() {
        let chains = crossed_chains();
        let order = arrange(&chains).expect("arrange");
        assert_eq!(crossings(&chains, &order.layers), 0);
        assert_eq!(order.layers, vec![vec![0, 1], vec![3, 2]]);
    }

    #[test]
    fn ports_follow_targets() {
        // a の右から c（上）と d（下）へ二本出る。
        let chains = Chains {
            items: vec![node(0), node(1), node(2)],
            layer: vec![0, 1, 1],
            layer_count: 2,
            links: vec![
                Link {
                    edge: 0,
                    left: 0,
                    right: 1,
                },
                Link {
                    edge: 1,
                    left: 0,
                    right: 2,
                },
            ],
            paths: vec![
                vec![Step {
                    link: 0,
                    forward: true,
                }],
                vec![Step {
                    link: 1,
                    forward: true,
                }],
            ],
        };
        let order = arrange(&chains).expect("arrange");
        assert!(order.east[0] < order.east[1]);
    }

    #[test]
    fn turn_ports_are_ordered() {
        // backward_makes_turns（付属書 L 4.5 節）の段を手で作る。
        // 0 a（層 2）、1 b（層 1）、2 TurnEast（層 3）、3 d(a)（層 2）、
        // 4 d(1)（層 1）、5 TurnWest（層 0）。
        let dummy = |edge: usize| Item {
            kind: ItemKind::Dummy(edge),
            size: Size {
                width: 0,
                height: 0,
            },
        };
        let turn_east = Item {
            kind: ItemKind::TurnEast(0),
            size: Size {
                width: 0,
                height: TURN_HEIGHT,
            },
        };
        let turn_west = Item {
            kind: ItemKind::TurnWest(0),
            size: Size {
                width: 0,
                height: TURN_HEIGHT,
            },
        };
        let link = |left: usize, right: usize| Link {
            edge: 0,
            left,
            right,
        };
        let chains = Chains {
            items: vec![node(0), node(1), turn_east, dummy(0), dummy(0), turn_west],
            layer: vec![2, 1, 3, 2, 1, 0],
            layer_count: 4,
            links: vec![link(0, 2), link(3, 2), link(4, 3), link(5, 4), link(5, 1)],
            paths: vec![vec![
                Step {
                    link: 0,
                    forward: true,
                },
                Step {
                    link: 1,
                    forward: false,
                },
                Step {
                    link: 2,
                    forward: false,
                },
                Step {
                    link: 3,
                    forward: false,
                },
                Step {
                    link: 4,
                    forward: true,
                },
            ]],
        };
        let order = arrange(&chains).expect("arrange");
        let mut west_turn = vec![order.west[0], order.west[1]];
        west_turn.sort();
        assert_eq!(west_turn, vec![0, TURN_HEIGHT]);
        let mut east_turn = vec![order.east[3], order.east[4]];
        east_turn.sort();
        assert_eq!(east_turn, vec![0, TURN_HEIGHT]);
    }
}
