//! Role: 層ごとの縦の座標を network simplex で決める（付属書 L の 7 章）。
use crate::level::{Chains, Item, ItemKind, Link, Order};
use crate::model::{LayoutError, MIXED_GAP, NODE_GAP, SMALL_GAP};
use crate::simplex::{self, Constraint, Problem};

/// item ごとの上辺の y。最小は 0。simplex の誤りはそのまま返す。
pub(crate) fn vertical(chains: &Chains, order: &Order) -> Result<Vec<i64>, LayoutError> {
    let item_count = chains.items.len();
    if item_count == 0 {
        return Ok(Vec::new());
    }
    check_references(chains, order)?;
    let mut edges = gap_edges(chains, order);
    edges.extend(link_edges(chains, order));
    let problem = Problem {
        node_count: item_count + chains.links.len(),
        edges,
    };
    let rank = simplex::solve(&problem)?;
    let items = &rank[..item_count];
    let least = items.iter().copied().min().unwrap_or(0);
    Ok(items.iter().map(|y| y - least).collect())
}

/// 層・port・link の参照が範囲内にあることを確かめる（外れていれば内部の矛盾）。
fn check_references(chains: &Chains, order: &Order) -> Result<(), LayoutError> {
    let item_count = chains.items.len();
    if order.east.len() != chains.links.len() || order.west.len() != chains.links.len() {
        return Err(LayoutError::Internal(
            "port count differs from the link count".to_string(),
        ));
    }
    if order.layers.iter().flatten().any(|&id| id >= item_count) {
        return Err(LayoutError::Internal(
            "layer refers to a missing item".to_string(),
        ));
    }
    if chains
        .links
        .iter()
        .any(|link| link.left >= item_count || link.right >= item_count)
    {
        return Err(LayoutError::Internal(
            "link refers to a missing item".to_string(),
        ));
    }
    Ok(())
}

/// 同じ層で上から隣り合う item の間隔（手順 2）。
fn gap_edges(chains: &Chains, order: &Order) -> Vec<Constraint> {
    let mut edges = Vec::new();
    for layer in &order.layers {
        for pair in layer.windows(2) {
            let upper = &chains.items[pair[0]];
            let lower = &chains.items[pair[1]];
            edges.push(Constraint {
                from: pair[0],
                to: pair[1],
                minlen: upper.size.height + gap(upper, lower),
                weight: 0,
            });
        }
    }
    edges
}

/// 上の item と下の item の間隔（手順 2 の表）。
fn gap(upper: &Item, lower: &Item) -> i64 {
    match (is_large(&upper.kind), is_large(&lower.kind)) {
        (true, true) => NODE_GAP,
        (true, false) | (false, true) => MIXED_GAP,
        (false, false) => SMALL_GAP,
    }
}

/// 大きな要素（node、box、label）。
fn is_large(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Node(_) | ItemKind::Box(_) | ItemKind::Label(_)
    )
}

/// 実体（node、box、入口、出口）。仮の要素（dummy、label、U ターン）と区別する。
fn is_real(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Node(_) | ItemKind::Box(_) | ItemKind::Entry(_) | ItemKind::Exit(_)
    )
}

/// link ごとに、port の高さを揃える二本の辺（手順 3）。link の node の index は item の数の後ろに続く。
fn link_edges(chains: &Chains, order: &Order) -> Vec<Constraint> {
    let item_count = chains.items.len();
    let mut edges = Vec::with_capacity(2 * chains.links.len());
    for (index, link) in chains.links.iter().enumerate() {
        let node = item_count + index;
        let east = order.east[index];
        let west = order.west[index];
        let center = east.max(west);
        let weight = link_weight(chains, link);
        edges.push(Constraint {
            from: node,
            to: link.left,
            minlen: center - east,
            weight,
        });
        edges.push(Constraint {
            from: node,
            to: link.right,
            minlen: center - west,
            weight,
        });
    }
    edges
}

/// link の重み ω(e)。両端が実体なら 1、片方が仮なら 2、両方が仮なら 8。
fn link_weight(chains: &Chains, link: &Link) -> i64 {
    let left = is_real(&chains.items[link.left].kind);
    let right = is_real(&chains.items[link.right].kind);
    match (left, right) {
        (true, true) => 1,
        (false, false) => 8,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::vertical;
    use crate::level::{Chains, Item, ItemKind, Link, Order};
    use crate::model::Size;

    fn item(kind: ItemKind, height: i64) -> Item {
        Item {
            kind,
            size: Size { width: 80, height },
        }
    }

    fn chains_of(items: Vec<Item>, layer: Vec<usize>, links: Vec<Link>) -> Chains {
        let layer_count = layer.iter().map(|layer| layer + 1).max().unwrap_or(0);
        Chains {
            items,
            layer,
            layer_count,
            links,
            paths: Vec::new(),
        }
    }

    #[test]
    fn aligns_ports() {
        let items = vec![item(ItemKind::Node(0), 20), item(ItemKind::Node(1), 40)];
        let links = vec![Link {
            edge: 0,
            left: 0,
            right: 1,
        }];
        let chains = chains_of(items, vec![0, 1], links);
        let order = Order {
            layers: vec![vec![0], vec![1]],
            east: vec![10],
            west: vec![20],
        };
        assert_eq!(vertical(&chains, &order), Ok(vec![10, 0]));
    }

    #[test]
    fn keeps_gap() {
        let items = vec![item(ItemKind::Node(0), 30), item(ItemKind::Node(1), 30)];
        let chains = chains_of(items, vec![0, 0], Vec::new());
        let order = Order {
            layers: vec![vec![0, 1]],
            east: Vec::new(),
            west: Vec::new(),
        };
        let y = vertical(&chains, &order).expect("the layout is feasible");
        assert!(y[1] - y[0] >= 70, "y = {y:?}");
    }

    #[test]
    fn straight_dummy_chain() {
        let items = vec![
            item(ItemKind::Node(0), 40),
            item(ItemKind::Dummy(0), 12),
            item(ItemKind::Node(1), 40),
        ];
        let links = vec![
            Link {
                edge: 0,
                left: 0,
                right: 1,
            },
            Link {
                edge: 0,
                left: 1,
                right: 2,
            },
        ];
        let chains = chains_of(items, vec![0, 1, 2], links);
        let order = Order {
            layers: vec![vec![0], vec![1], vec![2]],
            east: vec![20, 0],
            west: vec![0, 20],
        };
        let y = vertical(&chains, &order).expect("the layout is feasible");
        assert_eq!(y[0] + 20, y[1]);
        assert_eq!(y[2] + 20, y[1]);
    }
}
