use std::process::Command;
use xtask_layout::{
    Direction, EdgeRequest, EdgeStyle, LayoutRequest, NodeRequest, layout_with_node,
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
