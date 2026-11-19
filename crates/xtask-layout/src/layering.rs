//! Role: 段の node に層を割り当てる（付属書 L の 3 章）。
use std::collections::BTreeMap;

use crate::acyclic;
use crate::level::{Frame, Layers, LevelInput, RootLayering, Segment};
use crate::model::LayoutError;
use crate::simplex::{self, Constraint, Problem};

pub(crate) fn assign(input: &LevelInput) -> Result<Layers, LayoutError> {
    match &input.frame {
        Frame::Root(RootLayering::Fixed(columns)) => fixed(input.content_count(), columns),
        Frame::Root(RootLayering::Optimize) | Frame::Region { .. } => optimize(input),
    }
}

/// 列番号を詰め、列の順位の二倍を層にする（3.2 節）。
fn fixed(content: usize, columns: &[u32]) -> Result<Layers, LayoutError> {
    let used = columns
        .get(..content)
        .ok_or_else(|| LayoutError::Internal("fixed columns are fewer than the nodes".into()))?;
    let mut rank: BTreeMap<u32, i64> = used.iter().map(|&column| (column, 0)).collect();
    for (position, value) in rank.values_mut().enumerate() {
        *value = position as i64;
    }
    let layer = used.iter().map(|column| 2 * rank[column]).collect();
    Ok(Layers { layer })
}

/// 辺の総延長が最小になる層を求める（3.3 節）。
fn optimize(input: &LevelInput) -> Result<Layers, LayoutError> {
    let content = input.content_count();
    let segments: Vec<&Segment> = input
        .segments
        .iter()
        .filter(|segment| segment.from < content && segment.to < content)
        .collect();
    let arcs: Vec<(usize, usize, i64)> = segments
        .iter()
        .map(|segment| (segment.from, segment.to, 1))
        .collect();
    let position = acyclic::order(content, &arcs);
    let edges = segments
        .iter()
        .map(|segment| {
            let reversed = position[segment.from] > position[segment.to];
            let (from, to) = if reversed {
                (segment.to, segment.from)
            } else {
                (segment.from, segment.to)
            };
            let minlen = if segment.label.is_some() { 2 } else { 1 };
            Constraint {
                from,
                to,
                minlen,
                weight: 1,
            }
        })
        .collect();
    let problem = Problem {
        node_count: content,
        edges,
    };
    let ranks = simplex::solve(&problem)?;
    Ok(Layers {
        layer: balance(ranks, &problem),
    })
}

/// 均衡（3.3 節の手順 4）。入る辺と出る辺の重みが等しい node を、範囲の中で
/// 数の少ない層へ移す。移すのは、今の層より数が少ないときだけ。
fn balance(mut rank: Vec<i64>, problem: &Problem) -> Vec<i64> {
    let (inbound, outbound) = adjacency(problem);
    for node in 0..rank.len() {
        if weight_sum(&inbound[node]) != weight_sum(&outbound[node]) {
            continue;
        }
        let (low, high) = reach(node, &rank, &inbound, &outbound);
        let target = least_crowded(&rank, low, high);
        if crowd(&rank, target) < crowd(&rank, rank[node]) {
            rank[node] = target;
        }
    }
    rank
}

/// node ごとの入る辺と出る辺。
fn adjacency(problem: &Problem) -> (Vec<Vec<Constraint>>, Vec<Vec<Constraint>>) {
    let mut inbound = vec![Vec::new(); problem.node_count];
    let mut outbound = vec![Vec::new(); problem.node_count];
    for &arc in &problem.edges {
        outbound[arc.from].push(arc);
        inbound[arc.to].push(arc);
    }
    (inbound, outbound)
}

fn weight_sum(arcs: &[Constraint]) -> i64 {
    arcs.iter().map(|arc| arc.weight).sum()
}

/// 置ける層の範囲 [low, high]。入る辺が無ければ low は 0、出る辺が無ければ high は今の最大の層。
fn reach(
    node: usize,
    rank: &[i64],
    inbound: &[Vec<Constraint>],
    outbound: &[Vec<Constraint>],
) -> (i64, i64) {
    let low = inbound[node]
        .iter()
        .map(|arc| rank[arc.from] + arc.minlen)
        .max()
        .unwrap_or(0);
    let high = outbound[node]
        .iter()
        .map(|arc| rank[arc.to] - arc.minlen)
        .min()
        .unwrap_or_else(|| rank.iter().copied().max().unwrap_or(0));
    (low, high)
}

/// 範囲の中で node の数が最も少ない層（同数なら小さい層）。
fn least_crowded(rank: &[i64], low: i64, high: i64) -> i64 {
    let mut best = low;
    let mut best_count = crowd(rank, low);
    for layer in low + 1..=high {
        let count = crowd(rank, layer);
        if count < best_count {
            best = layer;
            best_count = count;
        }
    }
    best
}

/// 層にある node の数。
fn crowd(rank: &[i64], layer: i64) -> usize {
    rank.iter().filter(|&&value| value == layer).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{Item, ItemKind};
    use crate::model::Size;

    fn node(index: usize) -> Item {
        Item {
            kind: ItemKind::Node(index),
            size: Size {
                width: 10,
                height: 10,
            },
        }
    }

    fn entry(edge: usize) -> Item {
        Item {
            kind: ItemKind::Entry(edge),
            size: Size::default(),
        }
    }

    fn segment(from: usize, to: usize, label: Option<Size>) -> Segment {
        Segment {
            edge: 0,
            from,
            to,
            label,
        }
    }

    fn fixed(columns: &[u32]) -> LevelInput {
        LevelInput {
            items: (0..columns.len()).map(node).collect(),
            segments: Vec::new(),
            frame: Frame::Root(RootLayering::Fixed(columns.to_vec())),
        }
    }

    fn optimize(items: Vec<Item>, segments: Vec<Segment>) -> LevelInput {
        LevelInput {
            items,
            segments,
            frame: Frame::Root(RootLayering::Optimize),
        }
    }

    fn region(items: Vec<Item>, segments: Vec<Segment>) -> LevelInput {
        LevelInput {
            items,
            segments,
            frame: Frame::Region {
                header: Size::default(),
            },
        }
    }

    fn layer_of(input: &LevelInput) -> Vec<i64> {
        assign(input).expect("layers").layer
    }

    #[test]
    fn fixed_doubles_columns() {
        assert_eq!(layer_of(&fixed(&[0, 1, 1])), vec![0, 2, 2]);
    }

    #[test]
    fn fixed_columns_are_compressed() {
        assert_eq!(layer_of(&fixed(&[0, 1_000_000_000, 7])), vec![0, 4, 2]);
    }

    #[test]
    fn boundary_items_have_no_layer() {
        let input = region(vec![node(0), entry(0)], vec![segment(1, 0, None)]);
        assert_eq!(layer_of(&input).len(), 1);
    }

    #[test]
    fn chain_layers() {
        let input = region(
            vec![node(0), node(1), node(2)],
            vec![segment(0, 1, None), segment(1, 2, None)],
        );
        assert_eq!(layer_of(&input), vec![0, 1, 2]);
    }

    #[test]
    fn label_needs_two_layers() {
        let label = Some(Size {
            width: 10,
            height: 10,
        });
        let input = optimize(vec![node(0), node(1)], vec![segment(0, 1, label)]);
        assert_eq!(layer_of(&input), vec![0, 2]);
    }

    #[test]
    fn cycle_is_broken() {
        let input = optimize(
            vec![node(0), node(1)],
            vec![segment(0, 1, None), segment(1, 0, None)],
        );
        let layers = layer_of(&input);
        assert_ne!(layers[0], layers[1]);
    }

    #[test]
    fn balance_moves_isolated() {
        let input = optimize(
            vec![node(0), node(1), node(2), node(3)],
            vec![segment(0, 1, None)],
        );
        assert_eq!(layer_of(&input), vec![0, 1, 1, 0]);
    }
}
