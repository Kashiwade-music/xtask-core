//! Role: 実ELKの配置・接続・rankと、実行失敗の診断を検証する。
use std::process::Command;
use xtask_layout::{
    Direction, EdgeRequest, EdgeStyle, LayoutRequest, NodeRequest, REGION_FONT_SIZE,
    REGION_LINE_HEIGHT, RegionRequest, layout_with_node,
};

fn edge(id: &str, from: &str, to: &str, style: EdgeStyle) -> EdgeRequest {
    EdgeRequest {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        style,
        label: (style != EdgeStyle::Normal).then(|| format!("{style:?}: cfg(unix) 日本語")),
    }
}

#[test]
fn real_elk_keeps_normal_columns_with_same_rank_reverse_parallel_and_self_edges() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|result| result.status.success())
    {
        eprintln!("Node unavailable; optional real ELK rank test skipped");
        return;
    }
    let mut request = LayoutRequest {
        title: "ELK exact normal ranks".into(),
        direction: Direction::LeftToRight,
        nodes: ["a", "b", "c", "d", "isolated -> 日本語"]
            .into_iter()
            .map(|id| NodeRequest {
                id: id.into(),
                label: format!("node {id}"),
            })
            .collect(),
        regions: vec![],
        ranks: vec![
            vec!["a".into(), "b".into(), "isolated -> 日本語".into()],
            vec!["c".into()],
            vec!["d".into()],
        ],
        edges: vec![
            edge("ac", "a", "c", EdgeStyle::Normal),
            edge("bc", "b", "c", EdgeStyle::Normal),
            edge("cd", "c", "d", EdgeStyle::Normal),
            edge("ad", "a", "d", EdgeStyle::Normal),
        ],
    };
    for direction in [Direction::LeftToRight] {
        request.direction = direction;
        let baseline = layout_with_node(&request, &binary).unwrap();
        let mut augmented = request.clone();
        augmented.edges.extend([
            edge("same", "a", "b", EdgeStyle::Dev),
            edge("reverse", "d", "a", EdgeStyle::Dev),
            edge("parallel", "a", "c", EdgeStyle::Dev),
            edge("self", "b", "b", EdgeStyle::Dev),
            edge("build", "c", "b", EdgeStyle::Unconstrained),
        ]);
        let scene = layout_with_node(&augmented, &binary).unwrap();
        let coordinate = |node: &xtask_layout::SceneNode| match direction {
            Direction::LeftToRight => node.x,
            Direction::TopToBottom => node.y,
        };
        for scene in [&baseline, &scene] {
            let mut previous = None;
            for rank in &request.ranks {
                let positions: Vec<_> = rank
                    .iter()
                    .map(|id| coordinate(scene.nodes.iter().find(|node| node.id == *id).unwrap()))
                    .collect();
                assert!(
                    positions
                        .iter()
                        .all(|value| (*value - positions[0]).abs() < 0.001)
                );
                assert!(previous.is_none_or(|value| value < positions[0]));
                previous = Some(positions[0]);
            }
        }
        assert_eq!(scene.edges.len(), augmented.edges.len());
        assert!(
            scene
                .edges
                .iter()
                .filter(|edge| edge.style != EdgeStyle::Normal)
                .all(|edge| edge.label.is_some())
        );
        augmented.nodes.reverse();
        augmented.edges.reverse();
        for rank in &mut augmented.ranks {
            rank.reverse();
        }
        assert_eq!(layout_with_node(&augmented, &binary).unwrap(), scene);
    }
}

#[test]
fn invalid_inputs_fail_before_starting_node() {
    let mut request = LayoutRequest {
        title: String::new(),
        direction: Direction::LeftToRight,
        nodes: vec![NodeRequest {
            id: "a".into(),
            label: "a".into(),
        }],
        regions: vec![],
        edges: vec![],
        ranks: vec![vec!["a".into()]],
    };
    request
        .edges
        .push(edge("missing", "a", "absent", EdgeStyle::Normal));
    assert!(
        layout_with_node(&request, "missing-node-executable".as_ref())
            .unwrap_err()
            .contains("missing node")
    );
    request.edges.clear();
    request.ranks.push(vec!["a".into()]);
    assert!(
        layout_with_node(&request, "missing-node-executable".as_ref())
            .unwrap_err()
            .contains("duplicate ranked node")
    );
}

#[test]
fn real_elk_allocates_space_for_dense_native_and_ranked_ports() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|result| result.status.success())
    {
        return;
    }
    let sources: Vec<_> = (0..24).map(|index| format!("source{index:02}")).collect();
    let mut request = LayoutRequest {
        title: "dense fan-in".into(),
        direction: Direction::LeftToRight,
        nodes: sources
            .iter()
            .chain(std::iter::once(&"hub".to_owned()))
            .map(|id| NodeRequest {
                id: id.clone(),
                label: id.clone(),
            })
            .collect(),
        regions: vec![],
        ranks: vec![],
        edges: sources
            .iter()
            .enumerate()
            .map(|(index, id)| edge(&format!("edge{index:02}"), id, "hub", EdgeStyle::Normal))
            .collect(),
    };
    for ranked in [false, true] {
        if ranked {
            request.ranks = vec![sources.clone(), vec!["hub".into()]];
        }
        let scene = layout_with_node(&request, &binary).unwrap();
        assert!(
            scene
                .nodes
                .iter()
                .find(|node| node.id == "hub")
                .unwrap()
                .height
                >= 424.0
        );
        let mut ports: Vec<_> = scene
            .edges
            .iter()
            .map(|edge| edge.points.last().unwrap().y)
            .collect();
        ports.sort_by(f64::total_cmp);
        assert!(ports.windows(2).all(|pair| pair[1] - pair[0] >= 15.99));
    }
}

#[test]
fn real_elk_places_wrapped_region_headers_above_self_without_a_side_column() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|result| result.status.success())
    {
        eprintln!("Node unavailable; optional real ELK region-header test skipped");
        return;
    }
    let leaf = RegionRequest {
        id: "crate::leaf".into(),
        label: format!(
            "crate::leaf\n{}",
            "日本語の長い役割説明を読みやすく折り返して表示する。".repeat(3)
        ),
        nodes: vec!["leaf-self".into()],
        children: vec![],
    };
    let mut request = LayoutRequest {
        title: "Region headers".into(),
        direction: Direction::LeftToRight,
        nodes: vec![NodeRequest {
            id: "leaf-self".into(),
            label: "(self)".into(),
        }],
        regions: vec![leaf.clone()],
        edges: vec![],
        ranks: vec![],
    };
    for nested in [false, true] {
        if nested {
            request.nodes.push(NodeRequest {
                id: "root-self".into(),
                label: "(self)".into(),
            });
            request.regions = vec![RegionRequest {
                id: "crate".into(),
                label: "crate\n親領域の役割".into(),
                nodes: vec!["root-self".into()],
                children: vec![leaf.clone()],
            }];
        }
        let scene = layout_with_node(&request, &binary).unwrap();
        let region = scene
            .regions
            .iter()
            .find(|region| region.id == "crate::leaf")
            .unwrap();
        let node = scene
            .nodes
            .iter()
            .find(|node| node.id == "leaf-self")
            .unwrap();
        assert!(region.lines.len() > 3);
        assert_eq!(region.lines.concat(), leaf.label.replace('\n', ""));
        for line in &region.lines {
            let width: f64 = line
                .chars()
                .map(|character| REGION_FONT_SIZE * if character.is_ascii() { 0.62 } else { 1.05 })
                .sum();
            assert!(width <= 200.001, "header line exceeds 200px: {line:?}");
        }
        // The 200px header shares the node's horizontal area instead of creating
        // an additional side column; only the 20px left/right padding is needed.
        assert!(region.width <= 240.001, "leaf width {}", region.width);
        assert!(region.width >= node.width + 39.999);
        assert!(node.y >= region.y + region.lines.len() as f64 * REGION_LINE_HEIGHT + 19.999);
        assert!(
            node.x >= region.x + 19.999 && node.x + node.width <= region.x + region.width - 19.999
        );
        assert!(node.y + node.height <= region.y + region.height - 19.999);
        assert_eq!(region.parent.as_deref(), nested.then_some("crate"));
        if nested {
            let parent = scene
                .regions
                .iter()
                .find(|region| region.id == "crate")
                .unwrap();
            assert!(region.y >= parent.y + parent.lines.len() as f64 * REGION_LINE_HEIGHT + 19.999);
            assert!(region.x >= parent.x && region.x + region.width <= parent.x + parent.width);
        }
    }
}
