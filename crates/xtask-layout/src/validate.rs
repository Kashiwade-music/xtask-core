//! Role: 配置要求の整合性と、配置結果の形状・接続・階層を検証する。
use crate::model::{Direction, EdgeStyle, LayoutRequest, RegionRequest, Scene};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn request(request: &LayoutRequest) -> Result<(), String> {
    let mut nodes = BTreeSet::new();
    for node in &request.nodes {
        if !nodes.insert(node.id.as_str()) {
            return Err(format!("duplicate layout node {:?}", node.id));
        }
    }
    let mut region_ids = BTreeSet::new();
    let mut membership = BTreeSet::new();
    for region in &request.regions {
        regions(region, &nodes, &mut region_ids, &mut membership)?;
    }
    let mut edges = BTreeSet::new();
    for edge in &request.edges {
        if !edges.insert(&edge.id) {
            return Err(format!("duplicate layout edge {:?}", edge.id));
        }
        for id in [&edge.from, &edge.to] {
            if !nodes.contains(id.as_str()) {
                return Err(format!("layout edge references missing node {id:?}"));
            }
        }
    }
    if !request.ranks.is_empty() {
        if request.direction != Direction::LeftToRight {
            return Err("normal ranks require left-to-right layout with fixed side ports".into());
        }
        if !request.regions.is_empty() {
            return Err("normal ranks are supported for flat crate graphs only".into());
        }
        let mut ranked = BTreeMap::new();
        for (rank, ids) in request.ranks.iter().enumerate() {
            if ids.is_empty() {
                return Err("layout rank must not be empty".into());
            }
            for id in ids {
                if !nodes.contains(id.as_str()) {
                    return Err(format!("rank references missing node {id:?}"));
                }
                if ranked.insert(id.as_str(), rank).is_some() {
                    return Err(format!("duplicate ranked node {id:?}"));
                }
            }
        }
        if ranked.len() != nodes.len() {
            return Err("every crate node must have a normal rank".into());
        }
        for edge in request
            .edges
            .iter()
            .filter(|edge| edge.style == EdgeStyle::Normal)
        {
            if ranked[edge.from.as_str()] >= ranked[edge.to.as_str()] {
                return Err(format!(
                    "normal edge {:?} violates normal rank order",
                    edge.id
                ));
            }
        }
    }
    Ok(())
}

fn regions<'a>(
    region: &'a RegionRequest,
    nodes: &BTreeSet<&str>,
    ids: &mut BTreeSet<&'a str>,
    members: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if !ids.insert(&region.id) {
        return Err(format!("duplicate layout region {:?}", region.id));
    }
    for node in &region.nodes {
        if !nodes.contains(node.as_str()) {
            return Err(format!("region references missing node {node:?}"));
        }
        if !members.insert(node) {
            return Err(format!("node belongs to multiple regions {node:?}"));
        }
    }
    for child in &region.children {
        regions(child, nodes, ids, members)?;
    }
    Ok(())
}

pub(crate) fn scene(request: &LayoutRequest, scene: &Scene) -> Result<(), String> {
    let invalid = |message: &str| format!("invalid ELK scene: {message}");
    if scene.title != request.title
        || !scene.width.is_finite()
        || !scene.height.is_finite()
        || scene.width < 0.0
        || scene.height < 0.0
    {
        return Err(invalid("title or canvas dimensions do not match request"));
    }
    let nodes: BTreeMap<_, _> = scene
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    if nodes.len() != scene.nodes.len() || nodes.len() != request.nodes.len() {
        return Err(invalid("node IDs do not match request"));
    }
    for input in &request.nodes {
        let node = nodes
            .get(input.id.as_str())
            .ok_or_else(|| invalid("missing requested node"))?;
        bounds(scene, node.x, node.y, node.width, node.height)?;
        if node.label != input.label || node.lines.is_empty() {
            return Err(invalid("node label does not match request"));
        }
    }
    let mut expected_regions = BTreeMap::new();
    fn collect<'a>(
        regions: &'a [RegionRequest],
        parent: Option<&'a str>,
        out: &mut BTreeMap<&'a str, (&'a RegionRequest, Option<&'a str>)>,
    ) {
        for region in regions {
            out.insert(&region.id, (region, parent));
            collect(&region.children, Some(&region.id), out);
        }
    }
    collect(&request.regions, None, &mut expected_regions);
    let regions: BTreeMap<_, _> = scene
        .regions
        .iter()
        .map(|region| (region.id.as_str(), region))
        .collect();
    if regions.len() != scene.regions.len() || regions.len() != expected_regions.len() {
        return Err(invalid("region IDs do not match request"));
    }
    for (id, (input, parent)) in expected_regions {
        let region = regions
            .get(id)
            .ok_or_else(|| invalid("missing requested region"))?;
        bounds(scene, region.x, region.y, region.width, region.height)?;
        if region.label != input.label
            || region.parent.as_deref() != parent
            || region.lines.is_empty()
        {
            return Err(invalid("region label or parent does not match request"));
        }
        for node_id in &input.nodes {
            let node = nodes[node_id.as_str()];
            if !contains(
                [region.x, region.y, region.width, region.height],
                [node.x, node.y, node.width, node.height],
            ) {
                return Err(invalid("node coordinates fall outside its region"));
            }
        }
        if let Some(parent_id) = parent {
            let outer = regions[parent_id];
            if !contains(
                [outer.x, outer.y, outer.width, outer.height],
                [region.x, region.y, region.width, region.height],
            ) {
                return Err(invalid("child region coordinates fall outside parent"));
            }
        }
    }
    let edges: BTreeMap<_, _> = scene
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    if edges.len() != scene.edges.len() || edges.len() != request.edges.len() {
        return Err(invalid("edge IDs do not match request"));
    }
    for input in &request.edges {
        let edge = edges
            .get(input.id.as_str())
            .ok_or_else(|| invalid("missing requested edge"))?;
        if edge.from != input.from
            || edge.to != input.to
            || edge.style != input.style
            || edge.points.len() < 2
        {
            return Err(invalid("edge metadata does not match request"));
        }
        for point in &edge.points {
            if !point.x.is_finite()
                || !point.y.is_finite()
                || point.x < -0.001
                || point.y < -0.001
                || point.x > scene.width + 0.001
                || point.y > scene.height + 0.001
            {
                return Err(invalid("edge route falls outside canvas"));
            }
        }
        for pair in edge.points.windows(2) {
            if !close(pair[0].x, pair[1].x) && !close(pair[0].y, pair[1].y) {
                return Err(invalid("ELK route is not orthogonal"));
            }
        }
        let source = nodes[edge.from.as_str()];
        let target = nodes[edge.to.as_str()];
        let first = &edge.points[0];
        let next = &edge.points[1];
        let last = &edge.points[edge.points.len() - 1];
        let previous = &edge.points[edge.points.len() - 2];
        if !close(first.x, source.x + source.width)
            || first.y <= source.y
            || first.y >= source.y + source.height
            || !close(last.x, target.x)
            || last.y <= target.y
            || last.y >= target.y + target.height
            || !close(first.y, next.y)
            || next.x <= first.x
            || !close(last.y, previous.y)
            || last.x <= previous.x
        {
            return Err(invalid(&format!(
                "edge {:?} is not connected horizontally to rectangle sides",
                edge.id
            )));
        }
        match (&input.label, &edge.label) {
            (None, None) => (),
            (Some(text), Some(label)) if *text == label.text && !label.lines.is_empty() => bounds(
                scene,
                label.x - label.width / 2.0,
                label.y - label.height / 2.0,
                label.width,
                label.height,
            )?,
            _ => return Err(invalid("edge label does not match request")),
        }
    }
    let mut last_column = None;
    for rank in &request.ranks {
        let coordinate = |id: &str| match request.direction {
            Direction::LeftToRight => nodes[id].x,
            Direction::TopToBottom => nodes[id].y,
        };
        let column = coordinate(&rank[0]);
        if rank.iter().any(|id| !close(coordinate(id), column))
            || last_column.is_some_and(|previous| column <= previous)
        {
            return Err(invalid(
                "ELK placement changed normal rank membership or order",
            ));
        }
        last_column = Some(column);
    }
    Ok(())
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.001
}
fn contains([x, y, w, h]: [f64; 4], [inner_x, inner_y, inner_w, inner_h]: [f64; 4]) -> bool {
    inner_x >= x - 0.001
        && inner_y >= y - 0.001
        && inner_x + inner_w <= x + w + 0.001
        && inner_y + inner_h <= y + h + 0.001
}
fn bounds(scene: &Scene, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    if [x, y, width, height].iter().any(|value| !value.is_finite())
        || width < 0.0
        || height < 0.0
        || !contains([0.0, 0.0, scene.width, scene.height], [x, y, width, height])
    {
        return Err("invalid ELK scene: shape or label falls outside canvas".into());
    }
    Ok(())
}
