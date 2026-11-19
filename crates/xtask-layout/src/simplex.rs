//! Role: network simplex で差分制約の下の rank の重み付き和を最小にする。
use std::collections::BTreeSet;

use crate::model::LayoutError;

/// rank(to) - rank(from) >= minlen を表す辺。費用は weight * (rank(to) - rank(from))。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Constraint {
    pub from: usize,
    pub to: usize,
    pub minlen: i64,
    pub weight: i64,
}

pub(crate) struct Problem {
    pub node_count: usize,
    pub edges: Vec<Constraint>,
}

/// rank(to) - rank(from) >= minlen を満たす rank を返す。目的は Σ weight * (rank(to) - rank(from)) を
/// 小さくすること。繰返しの上限（手順 7）の中で最適に届けば最適解、届かなければその時点の実行可能解。
/// 辺は閉路を作らないこと。閉路・範囲外の端・負の weight は `Err(LayoutError::Internal(..))`。
/// 最小の rank は 0。
pub(crate) fn solve(problem: &Problem) -> Result<Vec<i64>, LayoutError> {
    let network = Network::new(problem)?;
    let mut rank = initial_ranks(&network)?;
    let mut tree_arc = feasible_tree(&network, &mut rank)?;
    let mut tree = Tree::new(&network, &tree_arc)?;
    let limit = 20 * problem.edges.len() + 100;
    let mut exchanges = 0;
    while let Some((_, leaving)) = leaving_arc(&tree) {
        if exchanges == limit {
            break;
        }
        exchanges += 1;
        let entering = entering_arc(&network, &tree, &tree_arc, &rank, leaving)?;
        tree_arc[leaving] = false;
        tree_arc[entering] = true;
        tree = Tree::new(&network, &tree_arc)?;
        rank = assign_ranks(&network, &tree);
    }
    Ok(normalize(&rank, problem.node_count))
}

/// 単純化し、根 `root` からすべての node への辺を足した問題（手順 1・2）。
struct Network {
    /// node の数（根を含む）。根は index `root`。
    nodes: usize,
    root: usize,
    /// 単純化した辺、続けて根の辺。
    arcs: Vec<Constraint>,
    /// node ごとに接する辺の index（昇順）。
    adj: Vec<Vec<usize>>,
}

impl Network {
    fn new(problem: &Problem) -> Result<Self, LayoutError> {
        let node_count = problem.node_count;
        for edge in &problem.edges {
            if edge.from >= node_count || edge.to >= node_count {
                return Err(LayoutError::Internal(
                    "simplex edge end is out of range".into(),
                ));
            }
            if edge.weight < 0 {
                return Err(LayoutError::Internal(
                    "simplex edge weight is negative".into(),
                ));
            }
        }
        let root = node_count;
        let mut arcs = simplify(&problem.edges);
        arcs.extend((0..node_count).map(|to| Constraint {
            from: root,
            to,
            minlen: 0,
            weight: 0,
        }));
        let mut adj = vec![Vec::new(); root + 1];
        for (index, arc) in arcs.iter().enumerate() {
            adj[arc.from].push(index);
            adj[arc.to].push(index);
        }
        Ok(Self {
            nodes: root + 1,
            root,
            arcs,
            adj,
        })
    }
}

/// 同じ (from, to) の辺を一本にまとめる。weight は和、minlen は最大（手順 1）。
fn simplify(edges: &[Constraint]) -> Vec<Constraint> {
    let mut sorted = edges.to_vec();
    sorted.sort_by_key(|a| (a.from, a.to));
    let mut merged: Vec<Constraint> = Vec::with_capacity(sorted.len());
    for edge in sorted {
        match merged.last_mut() {
            Some(last) if last.from == edge.from && last.to == edge.to => {
                last.weight += edge.weight;
                last.minlen = last.minlen.max(edge.minlen);
            }
            _ => merged.push(edge),
        }
    }
    merged
}

fn other_end(arc: &Constraint, node: usize) -> usize {
    if arc.from == node { arc.to } else { arc.from }
}

fn slack(arc: &Constraint, rank: &[i64]) -> i64 {
    rank[arc.to] - rank[arc.from] - arc.minlen
}

/// Kahn 法で初めの rank を求める（手順 3）。閉路があれば Err。
fn initial_ranks(network: &Network) -> Result<Vec<i64>, LayoutError> {
    let mut indegree = vec![0usize; network.nodes];
    for arc in &network.arcs {
        indegree[arc.to] += 1;
    }
    let mut ready: BTreeSet<usize> = (0..network.nodes)
        .filter(|&node| indegree[node] == 0)
        .collect();
    let mut rank = vec![0i64; network.nodes];
    let mut popped = 0;
    while let Some(node) = ready.pop_first() {
        popped += 1;
        for &index in &network.adj[node] {
            let arc = network.arcs[index];
            if arc.from != node {
                continue;
            }
            rank[arc.to] = rank[arc.to].max(rank[node] + arc.minlen);
            indegree[arc.to] -= 1;
            if indegree[arc.to] == 0 {
                ready.insert(arc.to);
            }
        }
    }
    if popped == network.nodes {
        Ok(rank)
    } else {
        Err(LayoutError::Internal(
            "simplex constraints contain a cycle".into(),
        ))
    }
}

/// 締まった辺で T の外の node を加え続ける（手順 4 の「加える」）。node は加えた順、辺は index の順に見る。
fn grow_tight(
    network: &Network,
    rank: &[i64],
    in_tree: &mut [bool],
    tree_arc: &mut [bool],
    members: &mut Vec<usize>,
) {
    let mut cursor = 0;
    while cursor < members.len() {
        let node = members[cursor];
        cursor += 1;
        for &index in &network.adj[node] {
            let arc = network.arcs[index];
            let other = other_end(&arc, node);
            if !in_tree[other] && slack(&arc, rank) == 0 {
                in_tree[other] = true;
                tree_arc[index] = true;
                members.push(other);
            }
        }
    }
}

/// T の一方の端だけにある辺のうち slack が最小のもの（同じなら index の最小）。
fn shortest_crossing(network: &Network, rank: &[i64], in_tree: &[bool]) -> Option<(usize, i64)> {
    let mut best: Option<(usize, i64)> = None;
    for (index, arc) in network.arcs.iter().enumerate() {
        if in_tree[arc.from] == in_tree[arc.to] {
            continue;
        }
        let value = slack(arc, rank);
        if best.is_none_or(|(_, current)| value < current) {
            best = Some((index, value));
        }
    }
    best
}

/// 実行可能な木を作る（手順 4）。rank を書き換え、木の辺の印を返す。
fn feasible_tree(network: &Network, rank: &mut [i64]) -> Result<Vec<bool>, LayoutError> {
    let mut in_tree = vec![false; network.nodes];
    let mut tree_arc = vec![false; network.arcs.len()];
    let mut members = vec![network.root];
    in_tree[network.root] = true;
    while members.len() < network.nodes {
        grow_tight(network, rank, &mut in_tree, &mut tree_arc, &mut members);
        if members.len() == network.nodes {
            break;
        }
        let (index, value) = shortest_crossing(network, rank, &in_tree)
            .ok_or_else(|| LayoutError::Internal("simplex graph is not connected".into()))?;
        let arc = network.arcs[index];
        let delta = if in_tree[arc.from] { value } else { -value };
        for &node in &members {
            rank[node] += delta;
        }
    }
    Ok(tree_arc)
}

/// 木の構造（手順 5・6）。すべて `Tree::new` が一度に作る。
struct Tree {
    /// node ごとの (親の node, 親との木の辺)。根は `None`。
    parent: Vec<Option<(usize, usize)>>,
    /// 後順の node の並び（子は親より前）。
    post: Vec<usize>,
    lim: Vec<i64>,
    low: Vec<i64>,
    /// 根以外の node の cut value（根は 0）。
    cut: Vec<i64>,
}

impl Tree {
    fn new(network: &Network, tree_arc: &[bool]) -> Result<Self, LayoutError> {
        let parent = tree_parents(network, tree_arc)?;
        let children = children_of(&parent);
        let (post, lim, low) = postorder(&children, network.root);
        let cut = cut_values(network, &parent, &post);
        Ok(Self {
            parent,
            post,
            lim,
            low,
            cut,
        })
    }
}

/// 根から木をたどって親を決める。
fn tree_parents(
    network: &Network,
    tree_arc: &[bool],
) -> Result<Vec<Option<(usize, usize)>>, LayoutError> {
    let mut parent = vec![None; network.nodes];
    let mut seen = vec![false; network.nodes];
    let mut queue = vec![network.root];
    seen[network.root] = true;
    let mut head = 0;
    while head < queue.len() {
        let node = queue[head];
        head += 1;
        for &index in &network.adj[node] {
            if !tree_arc[index] {
                continue;
            }
            let other = other_end(&network.arcs[index], node);
            if !seen[other] {
                seen[other] = true;
                parent[other] = Some((node, index));
                queue.push(other);
            }
        }
    }
    if queue.len() == network.nodes {
        Ok(parent)
    } else {
        Err(LayoutError::Internal(
            "simplex tree does not span the nodes".into(),
        ))
    }
}

/// 子の一覧。index の昇順。
fn children_of(parent: &[Option<(usize, usize)>]) -> Vec<Vec<usize>> {
    let mut children = vec![Vec::new(); parent.len()];
    for (node, link) in parent.iter().enumerate() {
        if let Some((up, _)) = link {
            children[*up].push(node);
        }
    }
    children
}

/// 後順で lim（1 から）と low を振る。子は index の昇順にたどる。
fn postorder(children: &[Vec<usize>], root: usize) -> (Vec<usize>, Vec<i64>, Vec<i64>) {
    let mut post = Vec::with_capacity(children.len());
    let mut lim = vec![0i64; children.len()];
    let mut low = vec![0i64; children.len()];
    let mut stack = vec![(root, 0usize)];
    while let Some(&(node, next)) = stack.last() {
        if next < children[node].len() {
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            stack.push((children[node][next], 0));
        } else {
            stack.pop();
            post.push(node);
            lim[node] = post.len() as i64;
            let value = children[node]
                .first()
                .map_or(lim[node], |&first| low[first]);
            low[node] = value;
        }
    }
    (post, lim, low)
}

/// 後順に cut value を求める（手順 6）。子の cut value は親より先に求まっている。
fn cut_values(network: &Network, parent: &[Option<(usize, usize)>], post: &[usize]) -> Vec<i64> {
    let mut cut = vec![0i64; network.nodes];
    for &node in post {
        let value = cut_value(network, parent, &cut, node);
        cut[node] = value;
    }
    cut
}

/// 根以外の node `child` の、親との木の辺の cut value。
fn cut_value(
    network: &Network,
    parent: &[Option<(usize, usize)>],
    cut: &[i64],
    child: usize,
) -> i64 {
    let Some((up, tree_arc)) = parent[child] else {
        return 0;
    };
    let arc = network.arcs[tree_arc];
    let child_is_tail = arc.from == child;
    let mut total = arc.weight;
    for &index in &network.adj[child] {
        if index == tree_arc {
            continue;
        }
        let other_arc = network.arcs[index];
        let is_out = other_arc.from == child;
        let other = other_end(&other_arc, child);
        if other == up {
            continue;
        }
        let points_to_head = is_out == child_is_tail;
        if points_to_head {
            total += other_arc.weight;
        } else {
            total -= other_arc.weight;
        }
        if matches!(parent[other], Some((owner, _)) if owner == child) {
            total += if points_to_head {
                -cut[other]
            } else {
                cut[other]
            };
        }
    }
    total
}

/// 負の cut value の木の辺のうち、子の index が最小のもの（子, 木の辺）。
fn leaving_arc(tree: &Tree) -> Option<(usize, usize)> {
    (0..tree.parent.len()).find_map(|node| match tree.parent[node] {
        Some((_, arc)) if tree.cut[node] < 0 => Some((node, arc)),
        _ => None,
    })
}

fn is_descendant(tree: &Tree, node: usize, ancestor: usize) -> bool {
    tree.low[ancestor] <= tree.lim[node] && tree.lim[node] <= tree.lim[ancestor]
}

/// 出る辺に替わる、木の外の辺を選ぶ（手順 7 の入る辺）。
fn entering_arc(
    network: &Network,
    tree: &Tree,
    tree_arc: &[bool],
    rank: &[i64],
    leaving: usize,
) -> Result<usize, LayoutError> {
    let arc = network.arcs[leaving];
    let tail = if tree.lim[arc.from] < tree.lim[arc.to] {
        arc.from
    } else {
        arc.to
    };
    let flip = tail == arc.to;
    let mut best: Option<(usize, i64)> = None;
    for (index, candidate) in network.arcs.iter().enumerate() {
        if tree_arc[index] {
            continue;
        }
        let crosses = is_descendant(tree, candidate.from, tail) == flip
            && is_descendant(tree, candidate.to, tail) != flip;
        if !crosses {
            continue;
        }
        let value = slack(candidate, rank);
        if best.is_none_or(|(_, current)| value < current) {
            best = Some((index, value));
        }
    }
    best.map(|(index, _)| index)
        .ok_or_else(|| LayoutError::Internal("no entering edge".into()))
}

/// 根から木の辺に沿って rank を決め直す（根の rank は 0）。
fn assign_ranks(network: &Network, tree: &Tree) -> Vec<i64> {
    let mut rank = vec![0i64; network.nodes];
    for &node in tree.post.iter().rev() {
        if let Some((up, index)) = tree.parent[node] {
            let arc = network.arcs[index];
            rank[node] = if arc.from == up {
                rank[up] + arc.minlen
            } else {
                rank[up] - arc.minlen
            };
        }
    }
    rank
}

/// 根を外し、最小の rank が 0 になるよう全体をずらす（手順 8）。
fn normalize(rank: &[i64], node_count: usize) -> Vec<i64> {
    let real = &rank[..node_count];
    let least = real.iter().copied().min().unwrap_or(0);
    real.iter().map(|value| value - least).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc(from: usize, to: usize, minlen: i64, weight: i64) -> Constraint {
        Constraint {
            from,
            to,
            minlen,
            weight,
        }
    }

    fn run(node_count: usize, edges: Vec<Constraint>) -> Result<Vec<i64>, LayoutError> {
        solve(&Problem { node_count, edges })
    }

    #[test]
    fn chain() {
        let ranks = run(3, vec![arc(0, 1, 1, 1), arc(1, 2, 1, 1)]).expect("solvable");
        assert_eq!(ranks, vec![0, 1, 2]);
    }

    #[test]
    fn moves_source_down() {
        let edges = vec![arc(0, 1, 1, 1), arc(1, 2, 1, 1), arc(3, 2, 1, 1)];
        let ranks = run(4, edges).expect("solvable");
        assert_eq!(ranks, vec![0, 1, 2, 1]);
    }

    #[test]
    fn respects_minlen() {
        let ranks = run(2, vec![arc(0, 1, 3, 1)]).expect("solvable");
        assert_eq!(ranks, vec![0, 3]);
    }

    #[test]
    fn zero_weight_free() {
        let ranks = run(2, vec![arc(0, 1, 1, 0)]).expect("solvable");
        assert!(ranks[1] - ranks[0] >= 1);
    }

    #[test]
    fn disconnected() {
        let ranks = run(3, Vec::new()).expect("solvable");
        assert_eq!(ranks, vec![0, 0, 0]);
    }

    #[test]
    fn offsets() {
        let edges = vec![arc(2, 0, 10, 1), arc(2, 1, 0, 1)];
        let ranks = run(3, edges).expect("solvable");
        assert_eq!(ranks, vec![10, 0, 0]);
        assert_eq!(ranks[0] - 10, ranks[1]);
    }

    #[test]
    fn cycle_is_internal_error() {
        let result = run(2, vec![arc(0, 1, 1, 1), arc(1, 0, 1, 1)]);
        assert!(matches!(result, Err(LayoutError::Internal(_))));
    }
}
