//! Role: 図のファイルの意味の規則を検査する。encode と decode が共有する唯一の検証。

use crate::model::{Archive, Content, Diagram, DiagramKind, Point, Rect, Scene};

/// 意味の検証。最初の違反を文で返す。
pub fn validate(archive: &Archive) -> Result<(), String> {
    let diagrams = &archive.diagrams;
    if diagrams.is_empty() {
        return Err("archive has no diagrams".into());
    }
    for index in 0..diagrams.len() {
        validate_diagram_at(diagrams, index)
            .map_err(|reason| format!("diagram {index}: {reason}"))?;
    }
    Ok(())
}

fn validate_diagram_at(diagrams: &[Diagram], index: usize) -> Result<(), String> {
    check_order(diagrams, index)?;
    check_content(&diagrams[index].content, diagrams.len())
}

/// 図の列の規則。先頭は crate 図、続きはすべて module 図で、kind が厳密に昇順。
fn check_order(diagrams: &[Diagram], index: usize) -> Result<(), String> {
    let kind = &diagrams[index].kind;
    if index == 0 {
        return if *kind == DiagramKind::Crates {
            Ok(())
        } else {
            Err("the first diagram must be the crate graph".into())
        };
    }
    if !matches!(kind, DiagramKind::Modules { .. }) {
        return Err("only the first diagram may be the crate graph".into());
    }
    if diagrams[index - 1].kind >= *kind {
        return Err("module diagrams must be in strictly ascending order".into());
    }
    Ok(())
}

fn check_content(content: &Content, diagram_count: usize) -> Result<(), String> {
    match content {
        Content::Drawn(scene) => check_scene(scene, diagram_count),
        Content::Failed(_) | Content::Skipped(_) => Ok(()),
    }
}

fn check_scene(scene: &Scene, diagram_count: usize) -> Result<(), String> {
    if scene.width < 0 || scene.height < 0 {
        return Err("scene size is negative".into());
    }
    check_items(scene, diagram_count)?;
    check_nodes(scene)?;
    check_regions(scene)?;
    check_edges(scene)?;
    check_coverage(scene)
}

fn check_items(scene: &Scene, diagram_count: usize) -> Result<(), String> {
    for item in &scene.items {
        if let Some(open) = item.open {
            if open == 0 || open as usize >= diagram_count {
                return Err(format!("open index {open} is out of range"));
            }
        }
    }
    Ok(())
}

fn check_nodes(scene: &Scene) -> Result<(), String> {
    for node in &scene.nodes {
        check_index(node.item, scene.items.len(), "node item")?;
        check_rect(&node.rect)?;
    }
    Ok(())
}

fn check_regions(scene: &Scene) -> Result<(), String> {
    for (index, region) in scene.regions.iter().enumerate() {
        check_index(region.item, scene.items.len(), "region item")?;
        check_rect(&region.rect)?;
        if let Some(parent) = region.parent {
            if parent as usize >= index {
                return Err(format!("region {index} has a parent that is not earlier"));
            }
        }
    }
    Ok(())
}

fn check_edges(scene: &Scene) -> Result<(), String> {
    for edge in &scene.edges {
        check_index(edge.from, scene.nodes.len(), "edge source node")?;
        check_index(edge.to, scene.nodes.len(), "edge target node")?;
        check_points(&edge.points)?;
        if let Some(label) = &edge.label {
            check_rect(&label.rect)?;
        }
    }
    Ok(())
}

/// 経路は二点以上の直交の線分で、最初と最後の線分は右向き。
fn check_points(points: &[Point]) -> Result<(), String> {
    if points.len() < 2 {
        return Err("edge has fewer than two points".into());
    }
    for pair in points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if (from.x != to.x) == (from.y != to.y) {
            return Err("edge segment is not horizontal or vertical, or has zero length".into());
        }
    }
    let last = points.len() - 1;
    if points[1].x <= points[0].x {
        return Err("the first edge segment must go right".into());
    }
    if points[last].x <= points[last - 1].x {
        return Err("the last edge segment must go right".into());
    }
    Ok(())
}

/// 当たり判定の規則。region があれば各 item は region から、無ければ node から、ちょうど一回参照される。
fn check_coverage(scene: &Scene) -> Result<(), String> {
    // 参照の数は item ごとの一バイト（飽和）。decode は items の列を読むときに、この分の費用を数える。
    let mut counts = vec![0u8; scene.items.len()];
    let what = if scene.regions.is_empty() {
        for node in &scene.nodes {
            count_reference(&mut counts, node.item);
        }
        "node"
    } else {
        for region in &scene.regions {
            count_reference(&mut counts, region.item);
        }
        "region"
    };
    match counts.iter().position(|&count| count != 1) {
        Some(index) => Err(format!(
            "item {index} is not referenced by exactly one {what}"
        )),
        None => Ok(()),
    }
}

fn count_reference(counts: &mut [u8], item: u32) {
    if let Some(count) = counts.get_mut(item as usize) {
        *count = count.saturating_add(1);
    }
}

fn check_index(index: u32, length: usize, what: &str) -> Result<(), String> {
    if (index as usize) < length {
        Ok(())
    } else {
        Err(format!("{what} index {index} is out of range"))
    }
}

fn check_rect(rect: &Rect) -> Result<(), String> {
    if rect.width < 0 || rect.height < 0 {
        Err("rectangle has a negative size".into())
    } else {
        Ok(())
    }
}
