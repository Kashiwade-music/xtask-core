//! Role: 長い辺にダミーを入れ、U ターンと空の層を整える（付属書 L の 4 章）。
use crate::level::{
    Chains, Frame, Item, ItemId, ItemKind, Layers, LevelInput, Link, Segment, Step,
};
use crate::model::{LayoutError, PORT_MARGIN, PORT_PITCH, Size, TURN_HEIGHT};

/// 段の入力と層の割当てから、ダミー・U ターン・空の層の整理を済ませた段を作る（4 章）。
pub(crate) fn build(input: &LevelInput, layers: &Layers) -> Result<Chains, LayoutError> {
    let span = span(input, layers);
    let draft = place(input, layers, &span)?;
    let kept = kept_layers(&draft, &span);
    let count = kept.iter().filter(|&&keep| keep).count();
    let draft = compact(draft, &kept);
    finish(input, draft, count)
}

/// 層の範囲（4.1 節）。`shift` を足した層が段の層になる。
struct Span {
    shift: i64,
    /// 層の数。region では Exit の層 + 1。
    count: i64,
    region: bool,
}

fn span(input: &LevelInput, layers: &Layers) -> Span {
    let mut need_min = layers.layer.iter().copied().min().unwrap_or(0);
    let mut need_max = layers.layer.iter().copied().max().unwrap_or(0);
    for segment in &input.segments {
        if is_boundary(input, segment) {
            continue;
        }
        let from = layers.layer[segment.from];
        let to = layers.layer[segment.to];
        if to <= from {
            need_min = need_min.min(to - 1);
            need_max = need_max.max(from + 1);
        }
    }
    match &input.frame {
        Frame::Root(_) => {
            let shift = -need_min;
            Span {
                shift,
                count: need_max + shift + 1,
                region: false,
            }
        }
        Frame::Region { .. } => {
            let shift = 1 - need_min;
            let exit = need_max + shift + 1;
            Span {
                shift,
                count: exit + 1,
                region: true,
            }
        }
    }
}

/// 境界の segment：from が Entry、または to が Exit。
fn is_boundary(input: &LevelInput, segment: &Segment) -> bool {
    matches!(input.items[segment.from].kind, ItemKind::Entry(_))
        || matches!(input.items[segment.to].kind, ItemKind::Exit(_))
}

/// ダミーを入れた途中の段。`layer` は shift 後・空の層の削除の前の層。
struct Draft {
    items: Vec<Item>,
    layer: Vec<i64>,
    /// 入力の segment ごとの道のり（item の列）。
    paths: Vec<Vec<ItemId>>,
}

impl Draft {
    fn new(input: &LevelInput, layers: &Layers, span: &Span) -> Self {
        let exit = span.count - 1;
        let layer = input
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| match item.kind {
                ItemKind::Entry(_) => 0,
                ItemKind::Exit(_) => exit,
                _ => layers.layer[index] + span.shift,
            })
            .collect();
        Self {
            items: input.items.clone(),
            layer,
            paths: Vec::with_capacity(input.segments.len()),
        }
    }

    fn add(&mut self, kind: ItemKind, size: Size, layer: i64) -> ItemId {
        self.items.push(Item { kind, size });
        self.layer.push(layer);
        self.items.len() - 1
    }

    /// 一本の segment の道のりを作り、item の列を返す（4.2 節）。
    fn route(&mut self, segment: &Segment) -> Result<Vec<ItemId>, LayoutError> {
        let start = self.layer[segment.from];
        let end = self.layer[segment.to];
        if end > start {
            self.forward(segment, start, end)
        } else {
            Ok(self.backward(segment, start, end))
        }
    }

    fn forward(
        &mut self,
        segment: &Segment,
        start: i64,
        end: i64,
    ) -> Result<Vec<ItemId>, LayoutError> {
        if segment.label.is_some() && end - start < 2 {
            return Err(LayoutError::Internal("label needs a free layer".into()));
        }
        let label_layer = start + (end - start) / 2;
        let mut path = vec![segment.from];
        for layer in start + 1..end {
            path.push(self.middle(segment, layer, label_layer));
        }
        path.push(segment.to);
        Ok(path)
    }

    fn backward(&mut self, segment: &Segment, start: i64, end: i64) -> Vec<ItemId> {
        let label_layer = start - (start - end) / 2;
        let mut path = vec![segment.from];
        let turn_east = ItemKind::TurnEast(segment.edge);
        path.push(self.add(turn_east, turn_size(), start + 1));
        for layer in (end..=start).rev() {
            path.push(self.middle(segment, layer, label_layer));
        }
        let turn_west = ItemKind::TurnWest(segment.edge);
        path.push(self.add(turn_west, turn_size(), end - 1));
        path.push(segment.to);
        path
    }

    /// 道の途中の item。ラベルの層なら Label、それ以外は Dummy。
    fn middle(&mut self, segment: &Segment, layer: i64, label_layer: i64) -> ItemId {
        match segment.label {
            Some(size) if layer == label_layer => {
                self.add(ItemKind::Label(segment.edge), size, layer)
            }
            _ => self.add(ItemKind::Dummy(segment.edge), Size::default(), layer),
        }
    }
}

fn turn_size() -> Size {
    Size {
        width: 0,
        height: TURN_HEIGHT,
    }
}

/// すべての segment の道のりを作る。
fn place(input: &LevelInput, layers: &Layers, span: &Span) -> Result<Draft, LayoutError> {
    let mut draft = Draft::new(input, layers, span);
    for segment in &input.segments {
        let path = draft.route(segment)?;
        draft.paths.push(path);
    }
    if draft
        .layer
        .iter()
        .any(|&layer| layer < 0 || layer >= span.count)
    {
        return Err(LayoutError::Internal("chain layer is out of range".into()));
    }
    Ok(draft)
}

/// 残す層（4.3 節）。非ダミーを含む層と、region の両端の層を残す。
/// それ以外（空の層、ダミーだけの層）は消す。
fn kept_layers(draft: &Draft, span: &Span) -> Vec<bool> {
    let count = span.count as usize;
    let mut kept = vec![false; count];
    for (item, &layer) in draft.items.iter().zip(&draft.layer) {
        if !matches!(item.kind, ItemKind::Dummy(_)) {
            kept[layer as usize] = true;
        }
    }
    if span.region {
        kept[0] = true;
        kept[count - 1] = true;
    }
    kept
}

/// 残さない層の dummy を消し、残す層を詰めて番号を振り直す。
/// 入力の item は先頭に同じ index で残る（消えるのは dummy だけ）。
fn compact(draft: Draft, kept: &[bool]) -> Draft {
    let mut renumber = vec![0i64; kept.len()];
    let mut next = 0;
    for (layer, &keep) in kept.iter().enumerate() {
        if keep {
            renumber[layer] = next;
            next += 1;
        }
    }
    let Draft {
        items: old_items,
        layer: old_layer,
        paths: old_paths,
    } = draft;
    let mut items = Vec::with_capacity(old_items.len());
    let mut layer = Vec::with_capacity(old_layer.len());
    let mut mapping: Vec<Option<ItemId>> = Vec::with_capacity(old_items.len());
    for (item, &old) in old_items.into_iter().zip(&old_layer) {
        if kept[old as usize] {
            mapping.push(Some(items.len()));
            items.push(item);
            layer.push(renumber[old as usize]);
        } else {
            mapping.push(None);
        }
    }
    let paths = old_paths
        .iter()
        .map(|path| path.iter().filter_map(|&id| mapping[id]).collect())
        .collect();
    Draft {
        items,
        layer,
        paths,
    }
}

/// 隣の層の二つの item を結ぶ link と、道のりの向き（4.2 節）。
fn orientation(
    layer: &[i64],
    first: ItemId,
    second: ItemId,
) -> Result<(ItemId, ItemId, bool), LayoutError> {
    match layer[second] - layer[first] {
        1 => Ok((first, second, true)),
        -1 => Ok((second, first, false)),
        _ => Err(LayoutError::Internal(
            "adjacent chain items are not in adjacent layers".into(),
        )),
    }
}

/// link と道のりの step を作り、node の高さを port の数に合わせる（4.4 節）。
fn finish(input: &LevelInput, draft: Draft, layer_count: usize) -> Result<Chains, LayoutError> {
    let mut links = Vec::new();
    let mut paths = Vec::with_capacity(draft.paths.len());
    for (segment, path) in input.segments.iter().zip(&draft.paths) {
        let mut steps = Vec::with_capacity(path.len().saturating_sub(1));
        for pair in path.windows(2) {
            let (left, right, forward) = orientation(&draft.layer, pair[0], pair[1])?;
            steps.push(Step {
                link: links.len(),
                forward,
            });
            links.push(Link {
                edge: segment.edge,
                left,
                right,
            });
        }
        paths.push(steps);
    }
    let mut items = draft.items;
    size_nodes(&mut items, &links);
    let layer = draft.layer.iter().map(|&value| value as usize).collect();
    Ok(Chains {
        items,
        layer,
        layer_count,
        links,
        paths,
    })
}

/// node の高さを、片側の port の数に合わせて広げる（4.4 節）。
fn size_nodes(items: &mut [Item], links: &[Link]) {
    let mut right_side = vec![0usize; items.len()];
    let mut left_side = vec![0usize; items.len()];
    for link in links {
        right_side[link.left] += 1;
        left_side[link.right] += 1;
    }
    for (index, item) in items.iter_mut().enumerate() {
        if matches!(item.kind, ItemKind::Node(_)) {
            let ports = right_side[index].max(left_side[index]) as i64;
            let needed = (ports + 1) * PORT_PITCH + PORT_MARGIN;
            item.size.height = item.size.height.max(needed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layering;
    use crate::level::{Frame, Item, ItemKind, RootLayering, Segment};
    use crate::model::Size;

    fn node(index: usize, height: i64) -> Item {
        Item {
            kind: ItemKind::Node(index),
            size: Size { width: 10, height },
        }
    }

    fn segment(edge: usize, from: usize, to: usize, label: Option<Size>) -> Segment {
        Segment {
            edge,
            from,
            to,
            label,
        }
    }

    fn fixed(items: Vec<Item>, segments: Vec<Segment>, columns: &[u32]) -> LevelInput {
        LevelInput {
            items,
            segments,
            frame: Frame::Root(RootLayering::Fixed(columns.to_vec())),
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

    fn run(input: &LevelInput) -> Chains {
        let layers = layering::assign(input).expect("layers");
        build(input, &layers).expect("chains")
    }

    fn dummy_layers(chains: &Chains) -> Vec<usize> {
        chains
            .items
            .iter()
            .zip(&chains.layer)
            .filter(|(item, _)| matches!(item.kind, ItemKind::Dummy(_)))
            .map(|(_, &layer)| layer)
            .collect()
    }

    fn directions(chains: &Chains, path: usize) -> Vec<bool> {
        chains.paths[path].iter().map(|step| step.forward).collect()
    }

    fn assert_links_adjacent(chains: &Chains) {
        for link in &chains.links {
            assert_eq!(chains.layer[link.left] + 1, chains.layer[link.right]);
        }
    }

    #[test]
    fn forward_span_three() {
        let items = vec![node(0, 10), node(1, 10), node(2, 10), node(3, 10)];
        let input = fixed(items, vec![segment(0, 0, 3, None)], &[0, 1, 2, 3]);
        let chains = run(&input);
        assert_eq!(chains.layer_count, 4);
        assert_eq!(dummy_layers(&chains), vec![1, 2]);
        assert_eq!(chains.links.len(), 3);
        assert_eq!(directions(&chains, 0), vec![true, true, true]);
        assert_links_adjacent(&chains);
    }

    #[test]
    fn backward_dummy_layer_removed() {
        let items = vec![node(0, 10), node(1, 10), node(2, 10)];
        let input = fixed(items, vec![segment(0, 2, 0, None)], &[0, 1, 2]);
        let chains = run(&input);
        assert_eq!(chains.layer_count, 5);
        assert_eq!(chains.paths[0].len(), 6);
        assert_eq!(
            directions(&chains, 0),
            vec![true, false, false, false, false, true]
        );
        assert_links_adjacent(&chains);
    }

    #[test]
    fn label_in_middle() {
        let label = Some(Size {
            width: 10,
            height: 10,
        });
        let items = vec![node(0, 10), node(1, 10)];
        let input = fixed(items, vec![segment(0, 0, 1, label)], &[0, 1]);
        let chains = run(&input);
        assert_eq!(chains.layer_count, 3);
        let labels: Vec<usize> = chains
            .items
            .iter()
            .zip(&chains.layer)
            .filter(|(item, _)| matches!(item.kind, ItemKind::Label(_)))
            .map(|(_, &layer)| layer)
            .collect();
        assert_eq!(labels, vec![1]);
    }

    #[test]
    fn backward_makes_turns() {
        let items = vec![node(0, 10), node(1, 10)];
        let input = LevelInput {
            items,
            segments: vec![segment(0, 0, 1, None)],
            frame: Frame::Root(RootLayering::Optimize),
        };
        let layers = Layers { layer: vec![1, 0] };
        let chains = build(&input, &layers).expect("chains");
        assert_eq!(chains.layer_count, 4);
        let turn_east = chains
            .items
            .iter()
            .zip(&chains.layer)
            .find(|(item, _)| matches!(item.kind, ItemKind::TurnEast(_)))
            .map(|(_, &layer)| layer);
        let turn_west = chains
            .items
            .iter()
            .zip(&chains.layer)
            .find(|(item, _)| matches!(item.kind, ItemKind::TurnWest(_)))
            .map(|(_, &layer)| layer);
        assert_eq!(turn_east, Some(3));
        assert_eq!(turn_west, Some(0));
        assert_eq!(chains.paths[0].len(), 5);
        assert_eq!(
            directions(&chains, 0),
            vec![true, false, false, false, true]
        );
    }

    #[test]
    fn dummy_only_layer_removed() {
        let items = vec![node(0, 10), node(1, 10)];
        let input = fixed(items, vec![segment(0, 0, 1, None)], &[0, 1]);
        let chains = run(&input);
        assert_eq!(chains.layer_count, 2);
        assert_eq!(chains.links.len(), 1);
    }

    #[test]
    fn node_height_grows() {
        let items = vec![node(0, 10), node(1, 10), node(2, 10), node(3, 10)];
        let segments = vec![
            segment(0, 0, 1, None),
            segment(1, 0, 2, None),
            segment(2, 0, 3, None),
        ];
        let input = fixed(items, segments, &[0, 1, 1, 1]);
        let chains = run(&input);
        assert_eq!(chains.items[0].size.height, 88);
    }

    #[test]
    fn region_keeps_boundary_layers() {
        let input = region(vec![node(0, 10)], Vec::new());
        let chains = run(&input);
        assert_eq!(chains.layer_count, 3);
    }
}
