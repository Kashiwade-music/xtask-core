//! Role: 重み付きの有向辺から、逆向きの辺が少なくなる node の順位を求める。

/// 重み付きの辺 (from, to, weight) の多重集合から、逆向きの辺の重みの和が小さくなる順位を返す。
/// 結果は node ごとの順位（0 から）。自己ループは無視する。
pub(crate) fn order(node_count: usize, edges: &[(usize, usize, i64)]) -> Vec<usize> {
    let mut rest = Remaining::new(node_count, edges);
    let mut front = Vec::with_capacity(node_count);
    let mut back = Vec::with_capacity(node_count);
    while rest.left > 0 {
        while let Some(node) = rest.first_sink() {
            rest.remove(node);
            back.push(node);
        }
        while let Some(node) = rest.first_source() {
            rest.remove(node);
            front.push(node);
        }
        if let Some(node) = rest.max_delta() {
            rest.remove(node);
            front.push(node);
        }
    }
    let sequence: Vec<usize> = front.into_iter().chain(back.into_iter().rev()).collect();
    let mut position = vec![0; node_count];
    for (rank, &node) in sequence.iter().enumerate() {
        position[node] = rank;
    }
    position
}

/// 残りの node と辺の重み（手順の R、out(v)、in(v)）。自己ループは除く。
struct Remaining {
    arcs: Vec<(usize, usize, i64)>,
    alive: Vec<bool>,
    out_weight: Vec<i64>,
    in_weight: Vec<i64>,
    left: usize,
}

impl Remaining {
    fn new(node_count: usize, edges: &[(usize, usize, i64)]) -> Self {
        let arcs: Vec<(usize, usize, i64)> = edges
            .iter()
            .copied()
            .filter(|&(from, to, _)| from != to)
            .collect();
        let mut out_weight = vec![0i64; node_count];
        let mut in_weight = vec![0i64; node_count];
        for &(from, to, weight) in &arcs {
            out_weight[from] += weight;
            in_weight[to] += weight;
        }
        Self {
            arcs,
            alive: vec![true; node_count],
            out_weight,
            in_weight,
            left: node_count,
        }
    }

    /// index が最小で out(v) == 0 の node。
    fn first_sink(&self) -> Option<usize> {
        (0..self.alive.len()).find(|&node| self.alive[node] && self.out_weight[node] == 0)
    }

    /// index が最小で in(v) == 0 の node。
    fn first_source(&self) -> Option<usize> {
        (0..self.alive.len()).find(|&node| self.alive[node] && self.in_weight[node] == 0)
    }

    /// out(v) - in(v) が最大の node（同じなら index の最小）。
    fn max_delta(&self) -> Option<usize> {
        let mut best: Option<(usize, i64)> = None;
        for node in 0..self.alive.len() {
            if !self.alive[node] {
                continue;
            }
            let delta = self.out_weight[node] - self.in_weight[node];
            if best.is_none_or(|(_, current)| delta > current) {
                best = Some((node, delta));
            }
        }
        best.map(|(node, _)| node)
    }

    /// node を残りから除き、残りの辺の重みを更新する。
    fn remove(&mut self, node: usize) {
        self.alive[node] = false;
        self.left -= 1;
        for &(from, to, weight) in &self.arcs {
            if from == node && self.alive[to] {
                self.in_weight[to] -= weight;
            }
            if to == node && self.alive[from] {
                self.out_weight[from] -= weight;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Role: 循環する辺を逆転させて閉路を除く処理（acyclic 化）の試験。
    use super::*;

    #[test]
    fn triangle() {
        let order = order(3, &[(0, 1, 1), (1, 2, 1), (2, 0, 1)]);
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn dag_keeps_order() {
        let order = order(3, &[(0, 2, 1), (1, 2, 1)]);
        assert!(order[0] < order[2]);
        assert!(order[1] < order[2]);
    }

    #[test]
    fn weights_matter() {
        let order = order(2, &[(0, 1, 5), (1, 0, 1)]);
        assert!(order[0] < order[1]);
    }
}
