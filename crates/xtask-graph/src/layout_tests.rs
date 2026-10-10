use super::*;
use std::{fs, process::Command};

fn example() -> Graph {
    Graph {
        title: "dependencies".into(),
        nodes: ["a", "b"]
            .into_iter()
            .map(|id| Node {
                id: id.into(),
                label: id.into(),
            })
            .collect(),
        edges: vec![
            Edge {
                from: "b".into(),
                to: "a".into(),
                label: None,
            },
            Edge {
                from: "a".into(),
                to: "b".into(),
                label: Some("uses".into()),
            },
            Edge {
                from: "a".into(),
                to: "b".into(),
                label: None,
            },
        ],
    }
}

#[test]
fn explicit_layout_validates_membership_and_keeps_styles_and_ranks_separate() {
    let graph = example();
    let layout = Layout {
        ranks: vec![vec!["a".into()], vec!["b".into()]],
        edge_styles: [(0, EdgeStyle::Unconstrained), (2, EdgeStyle::Dev)].into(),
        ..Default::default()
    };
    let request = graph.layout_request(&layout).unwrap();
    assert_eq!(request.ranks, layout.ranks);
    assert_eq!(request.direction, xtask_layout::Direction::LeftToRight);
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("uses")
                && edge.style == xtask_layout::EdgeStyle::Normal)
    );
    assert!(
        request
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("dev")
                && edge.style == xtask_layout::EdgeStyle::Dev)
    );
    assert!(request.edges.iter().any(|edge| edge.from == "b"
        && edge.to == "a"
        && edge.style == xtask_layout::EdgeStyle::Unconstrained));
    assert_eq!(
        request
            .edges
            .iter()
            .map(|edge| &edge.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    let mut reordered = graph.clone();
    reordered.edges.reverse();
    let mut reordered_layout = layout.clone();
    reordered_layout.edge_styles = [(2, EdgeStyle::Unconstrained), (0, EdgeStyle::Dev)].into();
    assert_eq!(
        request,
        reordered.layout_request(&reordered_layout).unwrap()
    );
    for invalid in [
        Layout {
            ranks: vec![vec!["missing".into()]],
            ..Default::default()
        },
        Layout {
            ranks: vec![vec!["a".into()], vec!["a".into()]],
            ..Default::default()
        },
        Layout {
            edge_styles: [(99, EdgeStyle::Dev)].into(),
            ..Default::default()
        },
        Layout {
            clusters: vec![Cluster {
                nodes: vec!["missing".into()],
                ..Default::default()
            }],
            ..Default::default()
        },
        Layout {
            clusters: vec![Cluster {
                id: "x".into(),
                children: vec![Cluster {
                    id: "x".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
        Layout {
            clusters: vec![Cluster {
                nodes: vec!["a".into()],
                children: vec![Cluster {
                    nodes: vec!["a".into()],
                    id: "child".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
    ] {
        assert!(graph.layout_request(&invalid).is_err());
    }
}

#[test]
fn real_elk_nested_regions_side_ports_and_dev_styles() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("Node not available; optional ELK layout integration check skipped");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let graph = Graph {
        title: "crate modules".into(),
        nodes: ["crate", "crate::a", "crate::a::leaf", "crate::b"]
            .into_iter()
            .map(|id| Node {
                id: id.into(),
                label: "(self)".into(),
            })
            .collect(),
        edges: vec![
            Edge {
                from: "crate".into(),
                to: "crate::a".into(),
                label: None,
            },
            Edge {
                from: "crate::a".into(),
                to: "crate::a::leaf".into(),
                label: None,
            },
            Edge {
                from: "crate::a::leaf".into(),
                to: "crate::b".into(),
                label: None,
            },
            Edge {
                from: "crate::a".into(),
                to: "crate".into(),
                label: Some("dev: cfg(test)".into()),
            },
            Edge {
                from: "crate::b".into(),
                to: "crate::b".into(),
                label: Some("dev".into()),
            },
        ],
    };
    let layout = Layout {
        direction: Direction::LeftToRight,
        clusters: vec![Cluster {
            id: "crate".into(),
            label: "crate".into(),
            nodes: vec!["crate".into()],
            children: vec![
                Cluster {
                    id: "a".into(),
                    label: "crate::a".into(),
                    nodes: vec!["crate::a".into()],
                    children: vec![Cluster {
                        id: "leaf".into(),
                        label: "crate::a::leaf".into(),
                        nodes: vec!["crate::a::leaf".into()],
                        children: vec![],
                    }],
                },
                Cluster {
                    id: "b".into(),
                    label: "crate::b".into(),
                    nodes: vec!["crate::b".into()],
                    children: vec![],
                },
            ],
        }],
        edge_styles: [(3, EdgeStyle::Dev), (4, EdgeStyle::Dev)].into(),
        ..Default::default()
    };
    let svg_path = directory.path().join("modules.svg");
    write_with_node_and_layout(&graph, &layout, &svg_path, &binary).unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    let rendered = fs::read_to_string(svg_path).unwrap();
    let document = roxmltree::Document::parse(&rendered).unwrap();
    assert_eq!(
        document.root_element().attribute("data-layout-engine"),
        Some("ELK")
    );
    let clusters: Vec<_> = document
        .descendants()
        .filter(|node| node.attribute("class") == Some("cluster"))
        .collect();
    assert_eq!(clusters.len(), 4);
    assert!(clusters.iter().any(|node| {
        node.ancestors()
            .skip(1)
            .filter(|parent| parent.attribute("class") == Some("cluster"))
            .count()
            == 2
    }));
    let nodes: std::collections::BTreeMap<_, _> = document
        .descendants()
        .filter(|node| node.attribute("class") == Some("node"))
        .map(|node| {
            (
                node.attribute("data-node-id").unwrap(),
                node.descendants()
                    .find(|rect| rect.has_tag_name("rect"))
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(nodes.len(), 4);
    let numeric =
        |node: roxmltree::Node<'_, '_>, key| node.attribute(key).unwrap().parse::<f64>().unwrap();
    let edges: Vec<_> = document
        .descendants()
        .filter(|node| node.attribute("class") == Some("edge"))
        .collect();
    assert_eq!(edges.len(), 5);
    assert_eq!(
        edges
            .iter()
            .filter(|edge| edge.attribute("data-style") == Some("dev"))
            .count(),
        2
    );
    for edge in edges {
        let source = nodes[edge.attribute("data-from").unwrap()];
        let target = nodes[edge.attribute("data-to").unwrap()];
        let sx = numeric(edge, "data-start-x");
        let sy = numeric(edge, "data-start-y");
        let tx = numeric(edge, "data-end-x");
        let ty = numeric(edge, "data-end-y");
        assert!((sx - numeric(source, "x") - numeric(source, "width")).abs() < 0.01);
        assert!((tx - numeric(target, "x")).abs() < 0.01);
        assert!(
            sy >= numeric(source, "y") && sy <= numeric(source, "y") + numeric(source, "height")
        );
        assert!(
            ty >= numeric(target, "y") && ty <= numeric(target, "y") + numeric(target, "height")
        );
        let path = edge
            .descendants()
            .find(|node| node.has_tag_name("path"))
            .unwrap();
        assert!(path.attribute("d").unwrap().starts_with('M'));
        assert!(
            !edge
                .descendants()
                .any(|node| node.attribute("stroke-dasharray").is_some())
        );
        let arrow = edge
            .descendants()
            .find(|node| node.attribute("class") == Some("arrowhead"))
            .unwrap();
        let tip = arrow
            .attribute("points")
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let (tip_x, tip_y) = tip.split_once(',').unwrap();
        assert!((tip_x.parse::<f64>().unwrap() - tx).abs() < 0.01);
        assert!((tip_y.parse::<f64>().unwrap() - ty).abs() < 0.01);
    }
}
