//! Role: 通路の track を割り当て、辺の折れ線を組み立てる（付属書 L の 8 章）。
use crate::acyclic;
use crate::level::{Chains, Frame, ItemKind, LevelInput, LevelOutput, Order, Piece, Step};
use crate::model::{
    LAYER_GAP, LayoutError, Point, REGION_PAD, Rect, Size, TRACK_GAP, TRACK_MARGIN,
};

pub(crate) fn finish(
    input: &LevelInput,
    chains: &Chains,
    order: &Order,
    y: &[i64],
) -> Result<LevelOutput, LayoutError> {
    check_shape(input, chains, order, y)?;
    let plan = Plan::new(&input.frame, chains, order, y)?;
    let rects: Vec<Rect> = (0..chains.items.len())
        .map(|item| plan.item_rect(item))
        .collect();
    let pieces = input
        .segments
        .iter()
        .enumerate()
        .map(|(segment, found)| plan.piece(segment, found.label))
        .collect::<Result<Vec<Piece>, LayoutError>>()?;
    let placement = plan.placement(&input.frame, &rects, &pieces);
    let mut rects: Vec<Rect> = rects
        .into_iter()
        .map(|rect| placement.moved_rect(rect))
        .collect();
    let pieces: Vec<Piece> = pieces
        .into_iter()
        .map(|piece| placement.moved_piece(piece))
        .collect();
    place_ports(&input.frame, chains, placement.size, &mut rects);
    rects.truncate(input.items.len());
    Ok(LevelOutput {
        size: placement.size,
        rects,
        pieces,
    })
}

/// 入力の形が配置の前提に合うか確かめる。合わなければ実装の誤りとして返す。
fn check_shape(
    input: &LevelInput,
    chains: &Chains,
    order: &Order,
    y: &[i64],
) -> Result<(), LayoutError> {
    let items = chains.items.len();
    let links = chains.links.len();
    let adjacent = chains.layer.len() == items
        && chains.links.iter().all(|link| {
            link.left < items
                && link.right < items
                && chains.layer[link.right] == chains.layer[link.left] + 1
        });
    let steps_valid = chains.paths.iter().flatten().all(|step| step.link < links);
    let consistent = input.items.len() <= items
        && chains.layer.len() == items
        && y.len() >= items
        && chains.paths.len() == input.segments.len()
        && order.east.len() == links
        && order.west.len() == links
        && chains.layer.iter().all(|&layer| layer < chains.layer_count)
        && adjacent
        && steps_valid;
    if consistent {
        Ok(())
    } else {
        Err(LayoutError::Internal(
            "route: the chains do not match the level".to_string(),
        ))
    }
}

/// 段の中の座標の計算の結果。track・横の座標・縦の座標を、一度だけ作って共有する。
struct Plan<'a> {
    chains: &'a Chains,
    order: &'a Order,
    y: &'a [i64],
    tracks: Tracks,
    columns: Columns,
}

impl<'a> Plan<'a> {
    fn new(
        frame: &Frame,
        chains: &'a Chains,
        order: &'a Order,
        y: &'a [i64],
    ) -> Result<Self, LayoutError> {
        let tracks = assign_tracks(chains, order, y);
        let columns = columns(frame, chains, &tracks)?;
        Ok(Self {
            chains,
            order,
            y,
            tracks,
            columns,
        })
    }

    /// 要素の左の x（層の左端と、層の中での中央揃え）。
    fn item_x(&self, item: usize) -> i64 {
        let layer = self.chains.layer[item];
        let width = self.columns.width[layer];
        self.columns.x[layer] + (width - self.chains.items[item].size.width) / 2
    }

    fn item_rect(&self, item: usize) -> Rect {
        let size = self.chains.items[item].size;
        Rect {
            x: self.item_x(item),
            y: self.y[item],
            width: size.width,
            height: size.height,
        }
    }

    /// 一つの道のりの step の点列。左から右へ進む向きで作り、逆向きなら反転する。
    fn step_points(&self, step: Step) -> Result<Vec<Point>, LayoutError> {
        let link = self.chains.links[step.link];
        let ya = self.y[link.left] + self.order.east[step.link];
        let yb = self.y[link.right] + self.order.west[step.link];
        let xa = self.item_x(link.left) + self.chains.items[link.left].size.width;
        let xb = self.item_x(link.right);
        let mut points = vec![Point { x: xa, y: ya }];
        if ya != yb {
            let track = self.tracks.track[step.link]
                .ok_or_else(|| LayoutError::Internal(format!("link {} has no track", step.link)))?;
            let gap = self.chains.layer[link.left];
            let x = self.columns.track_x[gap] + track as i64 * TRACK_GAP;
            points.push(Point { x, y: ya });
            points.push(Point { x, y: yb });
        }
        points.push(Point { x: xb, y: yb });
        if !step.forward {
            points.reverse();
        }
        Ok(points)
    }

    /// 区間（入力の segment）の折れ線と、その中のラベル。
    fn piece(&self, segment: usize, label: Option<Size>) -> Result<Piece, LayoutError> {
        let path = &self.chains.paths[segment];
        let mut points = Vec::new();
        for step in path {
            for point in self.step_points(*step)? {
                push_simplified(&mut points, point);
            }
        }
        let label_item = path
            .iter()
            .flat_map(|step| {
                let link = self.chains.links[step.link];
                [link.left, link.right]
            })
            .find(|&item| matches!(self.chains.items[item].kind, ItemKind::Label(_)));
        if label_item.is_some() != label.is_some() {
            return Err(LayoutError::Internal(format!(
                "segment {segment} has a label in the chains but not in the level, or the reverse"
            )));
        }
        Ok(Piece {
            points,
            label: label_item.map(|item| self.item_rect(item)),
        })
    }

    /// 段の原点の移動と大きさ（8.4 節）。
    fn placement(&self, frame: &Frame, rects: &[Rect], pieces: &[Piece]) -> Placement {
        let (min_x, min_y, max_x, max_y) = extent(rects, pieces).unwrap_or((0, 0, 0, 0));
        match frame {
            Frame::Root(_) => Placement {
                size: Size {
                    width: max_x - min_x,
                    height: max_y - min_y,
                },
                dx: -min_x,
                dy: -min_y,
            },
            Frame::Region { header } => {
                let width = self.columns.x.last().copied().unwrap_or(0);
                let inner = header.height + REGION_PAD;
                Placement {
                    size: Size {
                        width,
                        height: inner + (max_y - min_y) + REGION_PAD,
                    },
                    dx: 0,
                    dy: inner - min_y,
                }
            }
        }
    }
}

/// 段の原点の移動（`dx`、`dy`）と、段の大きさ。
struct Placement {
    size: Size,
    dx: i64,
    dy: i64,
}

impl Placement {
    fn moved_rect(&self, rect: Rect) -> Rect {
        Rect {
            x: rect.x + self.dx,
            y: rect.y + self.dy,
            ..rect
        }
    }

    fn moved_piece(&self, piece: Piece) -> Piece {
        Piece {
            points: piece
                .points
                .into_iter()
                .map(|point| Point {
                    x: point.x + self.dx,
                    y: point.y + self.dy,
                })
                .collect(),
            label: piece.label.map(|rect| self.moved_rect(rect)),
        }
    }
}

/// 点と矩形の外接（左、上、右、下）。何も無ければ `None`。
fn extent(rects: &[Rect], pieces: &[Piece]) -> Option<(i64, i64, i64, i64)> {
    let corners = rects
        .iter()
        .flat_map(|rect| {
            [
                (rect.x, rect.y),
                (rect.x + rect.width, rect.y + rect.height),
            ]
        })
        .chain(
            pieces
                .iter()
                .flat_map(|piece| piece.points.iter().map(|point| (point.x, point.y))),
        );
    corners.fold(None, |found, (x, y)| {
        Some(match found {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => {
                (left.min(x), top.min(y), right.max(x), bottom.max(y))
            }
        })
    })
}

/// 段の入口と出口の port は、幅と高さが 0 の点として置く（8.4 節）。
fn place_ports(frame: &Frame, chains: &Chains, size: Size, rects: &mut [Rect]) {
    if !matches!(frame, Frame::Region { .. }) {
        return;
    }
    for (item, rect) in chains.items.iter().zip(rects.iter_mut()) {
        let x = match item.kind {
            ItemKind::Entry(_) => 0,
            ItemKind::Exit(_) => size.width,
            _ => continue,
        };
        *rect = Rect {
            x,
            y: rect.y,
            width: 0,
            height: 0,
        };
    }
}

/// 同じ点を一つにし、同じ直線の上の三点では中の点を消す（8.3 節の 3）。
fn push_simplified(points: &mut Vec<Point>, next: Point) {
    if points.last() == Some(&next) {
        return;
    }
    let count = points.len();
    if count >= 2 && collinear(points[count - 2], points[count - 1], next) {
        points.pop();
        if points.last() == Some(&next) {
            return;
        }
    }
    points.push(next);
}

fn collinear(a: Point, b: Point, c: Point) -> bool {
    (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y)
}

/// 通路の一本の縦線（ya と yb の異なる link）。
#[derive(Clone, Copy, Debug)]
struct Channel {
    ya: i64,
    yb: i64,
    lo: i64,
    hi: i64,
}

/// track の割当て。`track[link]` は直線の link では `None`。`count[gap]` は通路の track の数 T。
#[derive(Debug, PartialEq, Eq)]
struct Tracks {
    track: Vec<Option<usize>>,
    count: Vec<usize>,
}

/// 横の座標（8.2 節）。
struct Columns {
    /// 層 L の左端の x（X(L)）。
    x: Vec<i64>,
    /// 層 L の幅（W(L)）。
    width: Vec<i64>,
    /// 通路 L の track 0 の x。track の無い通路では使わない。
    track_x: Vec<i64>,
}

fn channel(chains: &Chains, order: &Order, y: &[i64], link: usize) -> Option<Channel> {
    let edge = chains.links[link];
    let ya = y[edge.left] + order.east[link];
    let yb = y[edge.right] + order.west[link];
    (ya != yb).then(|| Channel {
        ya,
        yb,
        lo: ya.min(yb),
        hi: ya.max(yb),
    })
}

/// 通路（層 L と L+1 の間）ごとに track を割り当てる（8.1 節）。
fn assign_tracks(chains: &Chains, order: &Order, y: &[i64]) -> Tracks {
    let gaps = chains.layer_count.saturating_sub(1);
    let mut track = vec![None; chains.links.len()];
    let mut count = vec![0; gaps];
    for (gap, slot) in count.iter_mut().enumerate().take(gaps) {
        let channels: Vec<(usize, Channel)> = (0..chains.links.len())
            .filter(|&link| chains.layer[chains.links[link].left] == gap)
            .filter_map(|link| channel(chains, order, y, link).map(|found| (link, found)))
            .collect();
        let values: Vec<Channel> = channels.iter().map(|&(_, found)| found).collect();
        let tracks = gap_tracks(&values);
        for (&(link, _), &k) in channels.iter().zip(tracks.iter()) {
            track[link] = Some(k);
        }
        *slot = tracks.iter().max().map_or(0, |&k| k + 1);
    }
    Tracks { track, count }
}

/// 一つの通路の区画（channel）に track の番号を付ける。
fn gap_tracks(channels: &[Channel]) -> Vec<usize> {
    let mut edges = Vec::new();
    for s in 0..channels.len() {
        for t in s + 1..channels.len() {
            if !conflict(channels[s], channels[t]) {
                continue;
            }
            let s_left = left_cost(channels[s], channels[t]);
            let t_left = left_cost(channels[t], channels[s]);
            if s_left < t_left {
                edges.push((s, t, t_left - s_left));
            } else if s_left > t_left {
                edges.push((t, s, s_left - t_left));
            }
        }
    }
    let rank = acyclic::order(channels.len(), &edges);
    let mut by_rank: Vec<usize> = (0..channels.len()).collect();
    by_rank.sort_by_key(|&index| (rank[index], index));
    let mut track = vec![0; channels.len()];
    for (position, &t) in by_rank.iter().enumerate() {
        for &s in &by_rank[..position] {
            if conflict(channels[s], channels[t]) {
                track[t] = track[t].max(track[s] + 1);
            }
        }
    }
    track
}

/// 二つの区画が衝突する（間隔を空けて並べられない）。
fn conflict(s: Channel, t: Channel) -> bool {
    s.lo < t.hi + TRACK_GAP && t.lo < s.hi + TRACK_GAP
}

/// `a` を左の track に置いたときの費用（端の一致は数えない）。
fn left_cost(a: Channel, b: Channel) -> i64 {
    i64::from(a.lo < b.ya && b.ya < a.hi) + i64::from(b.lo < a.yb && a.yb < b.hi)
}

/// 通路ごとの基準の幅 C の、track を入れる前の値（8.2 節の 2）。
fn gap_widths(region: bool, counts: &[usize]) -> Vec<i64> {
    counts
        .iter()
        .enumerate()
        .map(|(gap, &count)| {
            let boundary = gap == 0 || gap + 1 == counts.len();
            let base = if region && boundary {
                REGION_PAD
            } else {
                LAYER_GAP
            };
            if count == 0 {
                base
            } else {
                base.max(2 * TRACK_MARGIN + (count as i64 - 1) * TRACK_GAP)
            }
        })
        .collect()
}

fn layer_widths(chains: &Chains) -> Vec<i64> {
    let mut width = vec![0; chains.layer_count];
    for (item, &layer) in chains.items.iter().zip(&chains.layer) {
        width[layer] = width[layer].max(item.size.width);
    }
    width
}

/// 層の左端 X(L) を、幅 W と通路の幅 C から作る。
fn layer_x(width: &[i64], gap: &[i64]) -> Vec<i64> {
    let mut x = Vec::with_capacity(width.len());
    let mut next = 0;
    for (layer, &w) in width.iter().enumerate() {
        x.push(next);
        next += w + gap.get(layer).copied().unwrap_or(0);
    }
    x
}

/// 横の座標を決める（8.2 節）。Region は両端の層の幅を、見出しの幅まで広げる。
fn columns(frame: &Frame, chains: &Chains, tracks: &Tracks) -> Result<Columns, LayoutError> {
    let region = matches!(frame, Frame::Region { .. });
    if region && chains.layer_count < 2 {
        return Err(LayoutError::Internal(
            "a region needs an entry layer and an exit layer".to_string(),
        ));
    }
    let width = layer_widths(chains);
    let mut gap = gap_widths(region, &tracks.count);
    if let Frame::Region { header } = frame {
        let needed = header.width + 2 * REGION_PAD;
        let right = layer_x(&width, &gap).last().copied().unwrap_or(0);
        if let Some(last) = gap.last_mut() {
            *last += (needed - right).max(0);
        }
    }
    let x = layer_x(&width, &gap);
    let track_x = gap
        .iter()
        .enumerate()
        .map(|(layer, &c)| {
            let count = tracks.count[layer];
            if count == 0 {
                0
            } else {
                x[layer] + width[layer] + (c - (count as i64 - 1) * TRACK_GAP) / 2
            }
        })
        .collect();
    Ok(Columns { x, width, track_x })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::{Frame, Item, ItemKind, Link, RootLayering, Segment, Step};
    use crate::model::Size;

    fn item(kind: ItemKind, width: i64, height: i64) -> Item {
        Item {
            kind,
            size: Size { width, height },
        }
    }

    fn link(edge: usize, left: usize, right: usize) -> Link {
        Link { edge, left, right }
    }

    fn forward(link: usize) -> Step {
        Step {
            link,
            forward: true,
        }
    }

    fn segment(edge: usize, from: usize, to: usize) -> Segment {
        Segment {
            edge,
            from,
            to,
            label: None,
        }
    }

    fn chains_of(
        items: &[Item],
        layer: Vec<usize>,
        layer_count: usize,
        links: Vec<Link>,
        paths: Vec<Vec<Step>>,
    ) -> Chains {
        Chains {
            items: items.to_vec(),
            layer,
            layer_count,
            links,
            paths,
        }
    }

    #[test]
    fn two_segments_need_two_tracks() {
        let items = vec![
            item(ItemKind::Node(0), 20, 20),
            item(ItemKind::Node(1), 20, 20),
            item(ItemKind::Node(2), 20, 20),
            item(ItemKind::Node(3), 20, 20),
        ];
        let input = LevelInput {
            items: items.clone(),
            segments: vec![segment(0, 0, 1), segment(1, 2, 3)],
            frame: Frame::Root(RootLayering::Optimize),
        };
        let chains = chains_of(
            &items,
            vec![0, 1, 0, 1],
            2,
            vec![link(0, 0, 1), link(1, 2, 3)],
            vec![vec![forward(0)], vec![forward(1)]],
        );
        let order = Order {
            layers: vec![vec![0, 2], vec![1, 3]],
            east: vec![0, 0],
            west: vec![0, 0],
        };
        let y = [0, 100, 50, 150];
        // s1 (y 0 から 100) と s2 (y 50 から 150) は重なるので、別の track に置く。
        // s2 が左の track 0、s1 が track 1 になる。
        assert_eq!(
            assign_tracks(&chains, &order, &y).track,
            vec![Some(1), Some(0)]
        );
        assert!(finish(&input, &chains, &order, &y).is_ok());
    }

    #[test]
    fn straight_link_has_no_track() {
        let items = vec![
            item(ItemKind::Node(0), 20, 20),
            item(ItemKind::Node(1), 20, 20),
        ];
        let input = LevelInput {
            items: items.clone(),
            segments: vec![segment(0, 0, 1)],
            frame: Frame::Root(RootLayering::Optimize),
        };
        let chains = chains_of(
            &items,
            vec![0, 1],
            2,
            vec![link(0, 0, 1)],
            vec![vec![forward(0)]],
        );
        let order = Order {
            layers: vec![vec![0], vec![1]],
            east: vec![0],
            west: vec![0],
        };
        let y = [0, 0];
        assert_eq!(assign_tracks(&chains, &order, &y).count, vec![0]);
        assert!(finish(&input, &chains, &order, &y).is_ok());
    }

    #[test]
    fn route_is_orthogonal() {
        let items = vec![
            item(ItemKind::Node(0), 20, 20),
            item(ItemKind::Node(1), 20, 20),
        ];
        let input = LevelInput {
            items: items.clone(),
            segments: vec![segment(0, 0, 1)],
            frame: Frame::Root(RootLayering::Optimize),
        };
        let chains = chains_of(
            &items,
            vec![0, 1],
            2,
            vec![link(0, 0, 1)],
            vec![vec![forward(0)]],
        );
        let order = Order {
            layers: vec![vec![0], vec![1]],
            east: vec![10],
            west: vec![10],
        };
        let out = finish(&input, &chains, &order, &[0, 100]).expect("a valid level");
        let points: Vec<(i64, i64)> = out.pieces[0].points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(points, vec![(20, 10), (44, 10), (44, 110), (68, 110)]);
        for pair in out.pieces[0].points.windows(2) {
            assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
        }
    }

    #[test]
    fn region_has_min_width() {
        let items = vec![
            item(ItemKind::Entry(0), 0, 0),
            item(ItemKind::Exit(0), 0, 0),
        ];
        let input = LevelInput {
            items: items.clone(),
            segments: vec![segment(0, 0, 1)],
            frame: Frame::Region {
                header: Size {
                    width: 300,
                    height: 16,
                },
            },
        };
        let chains = chains_of(
            &items,
            vec![0, 1],
            2,
            vec![link(0, 0, 1)],
            vec![vec![forward(0)]],
        );
        let order = Order {
            layers: vec![vec![0], vec![1]],
            east: vec![0],
            west: vec![0],
        };
        let out = finish(&input, &chains, &order, &[0, 0]).expect("a valid level");
        assert!(out.size.width >= 340);
    }
}
