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
/// 小さくすること。繰返しの上限の中で最適に届けば最適解、届かなければその時点の実行可能解。
/// 連結成分ごとに解き、各成分の最小の rank を 0 にする。
/// 閉路・範囲外の端・負の weight は `Err(LayoutError::Internal(..))`。
pub(crate) fn solve(problem: &Problem) -> Result<Vec<i64>, LayoutError> {
    for edge in &problem.edges {
        if edge.from >= problem.node_count || edge.to >= problem.node_count {
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
    let mut rank = vec![0i64; problem.node_count];
    for part in components(problem) {
        let ranks = solve_connected(&part.problem)?;
        for (local, &node) in part.nodes.iter().enumerate() {
            rank[node] = ranks[local];
        }
    }
    Ok(rank)
}

/// 一つの連結成分。`nodes` は元の node の index（昇順）、`problem` はその中の番号に付け替えた問題。
struct Part {
    nodes: Vec<usize>,
    problem: Problem,
}

/// 連結成分に分ける。成分は最小の node の昇順に並べる。
fn components(problem: &Problem) -> Vec<Part> {
    let count = problem.node_count;
    let mut adjacent = vec![Vec::new(); count];
    for edge in &problem.edges {
        adjacent[edge.from].push(edge.to);
        adjacent[edge.to].push(edge.from);
    }
    let mut part_of = vec![usize::MAX; count];
    let mut local = vec![0usize; count];
    let mut parts: Vec<Part> = Vec::new();
    for start in 0..count {
        if part_of[start] != usize::MAX {
            continue;
        }
        let index = parts.len();
        part_of[start] = index;
        let mut nodes = vec![start];
        let mut cursor = 0;
        while cursor < nodes.len() {
            let node = nodes[cursor];
            cursor += 1;
            for &other in &adjacent[node] {
                if part_of[other] == usize::MAX {
                    part_of[other] = index;
                    nodes.push(other);
                }
            }
        }
        nodes.sort_unstable();
        for (position, &node) in nodes.iter().enumerate() {
            local[node] = position;
        }
        let problem = Problem {
            node_count: nodes.len(),
            edges: Vec::new(),
        };
        parts.push(Part { nodes, problem });
    }
    for edge in &problem.edges {
        let part = &mut parts[part_of[edge.from]];
        part.problem.edges.push(Constraint {
            from: local[edge.from],
            to: local[edge.to],
            ..*edge
        });
    }
    parts
}

/// 連結な問題を解く（手順 3〜8）。木の根は node 0。
fn solve_connected(problem: &Problem) -> Result<Vec<i64>, LayoutError> {
    let network = Network::new(problem);
    let mut rank = initial_ranks(&network)?;
    let tree_arc = feasible_tree(&network, &mut rank)?;
    let mut tree = Tree::new(&network, tree_arc, rank)?;
    let limit = 20 * problem.edges.len() + 100;
    for _ in 0..limit {
        let Some(leaving) = tree.leaving_arc() else {
            break;
        };
        let entering = tree.entering_arc(&network, leaving)?;
        tree.exchange(&network, leaving, entering)?;
    }
    Ok(normalize(&tree.rank, problem.node_count))
}

/// 単純化した連結な問題（手順 1）。木の根は node 0。
struct Network {
    nodes: usize,
    root: usize,
    /// 単純化した辺。
    arcs: Vec<Constraint>,
    /// node ごとに接する辺の index（昇順）。
    adj: Vec<Vec<usize>>,
}

impl Network {
    fn new(problem: &Problem) -> Self {
        let arcs = simplify(&problem.edges);
        let mut adj = vec![Vec::new(); problem.node_count];
        for (index, arc) in arcs.iter().enumerate() {
            adj[arc.from].push(index);
            adj[arc.to].push(index);
        }
        Self {
            nodes: problem.node_count,
            root: 0,
            arcs,
            adj,
        }
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

/// 一度に調べる負の cut value の木の辺の数（手順 7 の出る辺）。
const SEARCH_SIZE: usize = 30;

/// 木と rank。交換のたびに、変わる部分だけを直す（手順 5〜7）。
struct Tree {
    rank: Vec<i64>,
    /// 辺ごとの、木の辺かどうか。
    in_tree: Vec<bool>,
    /// 木の辺の並び。交換では、出る辺の位置に入る辺を置く。
    list: Vec<usize>,
    /// 出る辺の探索を続ける、`list` の位置。
    cursor: usize,
    /// node ごとの親との木の辺。根は None。
    parent: Vec<Option<usize>>,
    low: Vec<usize>,
    lim: Vec<usize>,
    /// 木の辺ごとの cut value（木の外の辺は 0）。
    cut: Vec<i64>,
}

impl Tree {
    fn new(network: &Network, in_tree: Vec<bool>, rank: Vec<i64>) -> Result<Self, LayoutError> {
        let list: Vec<usize> = (0..in_tree.len()).filter(|&arc| in_tree[arc]).collect();
        if list.len() + 1 != network.nodes {
            return Err(LayoutError::Internal(
                "simplex tree does not span the nodes".into(),
            ));
        }
        let mut tree = Self {
            rank,
            in_tree,
            list,
            cursor: 0,
            parent: vec![None; network.nodes],
            low: vec![0; network.nodes],
            lim: vec![0; network.nodes],
            cut: vec![0; network.arcs.len()],
        };
        let post = tree.number(network, network.root, None, 1);
        if post.len() != network.nodes {
            return Err(LayoutError::Internal(
                "simplex tree does not span the nodes".into(),
            ));
        }
        for &node in &post {
            if let Some(arc) = tree.parent[node] {
                tree.cut[arc] = tree.cut_value(network, node, arc);
            }
        }
        Ok(tree)
    }

    /// `top` の部分木の parent・low・lim を後順で振り直す（lim は `first` から）。後順の node の列を返す。
    fn number(
        &mut self,
        network: &Network,
        top: usize,
        up: Option<usize>,
        first: usize,
    ) -> Vec<usize> {
        let mut post = Vec::new();
        let mut stack = vec![(top, up, 0usize)];
        self.parent[top] = up;
        self.low[top] = first;
        let mut next = first;
        while let Some(frame) = stack.last_mut() {
            let (node, via) = (frame.0, frame.1);
            let adjacency = &network.adj[node];
            let mut child = None;
            while frame.2 < adjacency.len() {
                let arc = adjacency[frame.2];
                frame.2 += 1;
                if self.in_tree[arc] && Some(arc) != via {
                    child = Some((other_end(&network.arcs[arc], node), arc));
                    break;
                }
            }
            match child {
                Some((other, arc)) => {
                    self.parent[other] = Some(arc);
                    self.low[other] = next;
                    stack.push((other, Some(arc), 0));
                }
                None => {
                    stack.pop();
                    self.lim[node] = next;
                    next += 1;
                    post.push(node);
                }
            }
        }
        post
    }

    /// node `child` と親を結ぶ木の辺 `tree_arc` の cut value（手順 6）。子の側の木の辺の cut value は求まっていること。
    fn cut_value(&self, network: &Network, child: usize, tree_arc: usize) -> i64 {
        let arc = network.arcs[tree_arc];
        let child_is_tail = arc.from == child;
        let mut total = arc.weight;
        for &index in &network.adj[child] {
            if index == tree_arc {
                continue;
            }
            let other_arc = network.arcs[index];
            let points_to_head = (other_arc.from == child) == child_is_tail;
            total += if points_to_head {
                other_arc.weight
            } else {
                -other_arc.weight
            };
            if self.in_tree[index] {
                total += if points_to_head {
                    -self.cut[index]
                } else {
                    self.cut[index]
                };
            }
        }
        total
    }

    /// 負の cut value の木の辺を、前回の位置から順に探す。見つけた最初の SEARCH_SIZE 個のうち
    /// 最も負のもの（同じなら先に見つけたもの）。
    fn leaving_arc(&mut self) -> Option<usize> {
        let count = self.list.len();
        let mut best: Option<(usize, i64)> = None;
        let mut found = 0;
        for step in 0..count {
            let position = (self.cursor + step) % count;
            let value = self.cut[self.list[position]];
            if value < 0 {
                if best.is_none_or(|(_, least)| value < least) {
                    best = Some((position, value));
                }
                found += 1;
                if found == SEARCH_SIZE {
                    break;
                }
            }
        }
        let (position, _) = best?;
        self.cursor = position;
        Some(self.list[position])
    }

    /// `node` が `top` の部分木にあるか。
    fn contains(&self, top: usize, node: usize) -> bool {
        self.low[top] <= self.lim[node] && self.lim[node] <= self.lim[top]
    }

    /// 出る辺で切った下側の部分木（`lim` の小さい端の側）と上側をつなぐ木の外の辺のうち、
    /// slack が最小のもの（同じなら index の最小）。下側が出る辺の from の側なら上側から下側へ入る辺を、
    /// to の側なら下側から上側へ出る辺を探す。下側の部分木だけをたどる。
    fn entering_arc(&self, network: &Network, leaving: usize) -> Result<usize, LayoutError> {
        let arc = network.arcs[leaving];
        let (top, outward) = if self.lim[arc.from] < self.lim[arc.to] {
            (arc.from, false)
        } else {
            (arc.to, true)
        };
        let mut best: Option<(usize, i64)> = None;
        let mut stack = vec![top];
        while let Some(node) = stack.pop() {
            for &index in &network.adj[node] {
                let candidate = network.arcs[index];
                if self.in_tree[index] {
                    let other = other_end(&candidate, node);
                    if self.parent[other] == Some(index) {
                        stack.push(other);
                    }
                    continue;
                }
                let (inside, outside) = if outward {
                    (candidate.from, candidate.to)
                } else {
                    (candidate.to, candidate.from)
                };
                if inside != node || self.contains(top, outside) {
                    continue;
                }
                let value = slack(&candidate, &self.rank);
                let better = best.is_none_or(|(current, least)| {
                    value < least || value == least && index < current
                });
                if better {
                    best = Some((index, value));
                }
            }
        }
        best.map(|(index, _)| index)
            .ok_or_else(|| LayoutError::Internal("no entering edge".into()))
    }

    /// 木の辺 `leaving` を外し、`entering` を入れる。rank・cut value・parent・low・lim のうち、
    /// 変わる部分だけを直す。
    fn exchange(
        &mut self,
        network: &Network,
        leaving: usize,
        entering: usize,
    ) -> Result<(), LayoutError> {
        let out = network.arcs[leaving];
        let inn = network.arcs[entering];
        let delta = slack(&inn, &self.rank);
        if delta > 0 {
            if self.lim[out.from] < self.lim[out.to] {
                self.shift(network, out.from, leaving, -delta);
            } else {
                self.shift(network, out.to, leaving, delta);
            }
        }
        let value = self.cut[leaving];
        let lca = self.climb(network, inn.from, inn.to, value, true);
        if self.climb(network, inn.to, inn.from, value, false) != lca {
            return Err(LayoutError::Internal(
                "simplex tree update lost its common ancestor".into(),
            ));
        }
        self.cut[entering] = -value;
        self.cut[leaving] = 0;
        self.in_tree[leaving] = false;
        self.in_tree[entering] = true;
        let position = self
            .list
            .iter()
            .position(|&arc| arc == leaving)
            .ok_or_else(|| LayoutError::Internal("leaving arc is not in the tree".into()))?;
        self.list[position] = entering;
        let (up, first) = (self.parent[lca], self.low[lca]);
        self.number(network, lca, up, first);
        Ok(())
    }

    /// 木の辺 `via` の下の部分木 `top` の rank に `delta` を足す。
    fn shift(&mut self, network: &Network, top: usize, via: usize, delta: i64) {
        let mut stack = vec![(top, via)];
        while let Some((node, from)) = stack.pop() {
            self.rank[node] += delta;
            for &index in &network.adj[node] {
                if self.in_tree[index] && index != from {
                    stack.push((other_end(&network.arcs[index], node), index));
                }
            }
        }
    }

    /// `node` から、`other` を部分木に含む祖先まで上り、途中の木の辺の cut value に `value` を足すか引く。
    /// `forward` は、上る道の向きと入る辺の向きが同じとき足す側。その祖先を返す。
    fn climb(
        &mut self,
        network: &Network,
        mut node: usize,
        other: usize,
        value: i64,
        forward: bool,
    ) -> usize {
        while !self.contains(node, other) {
            let Some(arc) = self.parent[node] else {
                break;
            };
            let edge = network.arcs[arc];
            let add = (node == edge.from) == forward;
            self.cut[arc] += if add { value } else { -value };
            node = if self.lim[edge.from] > self.lim[edge.to] {
                edge.from
            } else {
                edge.to
            };
        }
        node
    }
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
