use super::*;

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
    let dot = graph.dot_with_layout(&layout).unwrap();
    assert!(dot.contains("\"a\" -> \"b\" [label=\"uses\"];"));
    assert!(dot.contains("label=\"dev\", constraint=false, id=\"xtask-dev-edge-0\""));
    assert!(dot.contains("\"b\" -> \"a\" [constraint=false]"));
    assert!(dot.contains("{ rank=same; \"a\"; }"));
    let mut reordered = graph.clone();
    reordered.edges.reverse();
    let mut reordered_layout = layout.clone();
    reordered_layout.edge_styles = [(2, EdgeStyle::Unconstrained), (0, EdgeStyle::Dev)].into();
    assert_eq!(dot, reordered.dot_with_layout(&reordered_layout).unwrap());
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
        assert!(graph.dot_with_layout(&invalid).is_err());
    }
}

#[test]
fn failed_wave_processing_keeps_previous_svg() {
    let directory = tempfile::tempdir().unwrap();
    let svg_path = directory.path().join("old.svg");
    let dot_path = directory.path().join("nodes.dot");
    fs::write(&svg_path, "previous SVG").unwrap();
    let source = directory.path().join("fake_dot.rs");
    fs::write(&source, r#"fn main() { let args: Vec<_> = std::env::args_os().collect(); std::fs::write(&args[4], "<svg/>").unwrap(); }"#).unwrap();
    let binary = directory
        .path()
        .join(format!("fake-dot{}", std::env::consts::EXE_SUFFIX));
    let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(source)
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap();
    assert!(status.success());
    let layout = Layout {
        edge_styles: [(0, EdgeStyle::Dev)].into(),
        ..Default::default()
    };
    let error = write_with_dot_and_layout(
        &example(),
        &layout,
        &dot_path,
        &svg_path,
        binary.as_os_str(),
    )
    .unwrap_err();
    assert!(error.contains("expected one SVG dev edge"));
    assert_eq!(fs::read_to_string(&svg_path).unwrap(), "previous SVG");
    assert_eq!(
        fs::read_to_string(dot_path).unwrap(),
        example().dot_with_layout(&layout).unwrap()
    );
}

#[test]
fn real_graphviz_nested_regions_and_true_waves() {
    let binary = std::env::var_os("XTASK_GRAPHVIZ_DOT").unwrap_or_else(|| "dot".into());
    if !Command::new(&binary)
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("Graphviz not available; optional wave integration check skipped");
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
                label: None,
            },
            Edge {
                from: "crate".into(),
                to: "crate::a".into(),
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
        edge_styles: [
            (3, EdgeStyle::Dev),
            (4, EdgeStyle::Dev),
            (5, EdgeStyle::Dev),
        ]
        .into(),
        ..Default::default()
    };
    let dot_path = directory.path().join("modules.dot");
    let raw_path = directory.path().join("raw.svg");
    let svg_path = directory.path().join("modules.svg");
    fs::write(&dot_path, graph.dot_with_layout(&layout).unwrap()).unwrap();
    let raw_render = Command::new(&binary)
        .arg("-Tsvg")
        .arg(&dot_path)
        .arg("-o")
        .arg(&raw_path)
        .output()
        .unwrap();
    assert!(
        raw_render.status.success(),
        "{}",
        String::from_utf8_lossy(&raw_render.stderr)
    );
    write_with_dot_and_layout(&graph, &layout, &dot_path, &svg_path, &binary).unwrap();
    let original = fs::read_to_string(raw_path).unwrap();
    let rendered = fs::read_to_string(svg_path).unwrap();
    let options = || roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let original_doc = roxmltree::Document::parse_with_options(&original, options()).unwrap();
    let rendered_doc = roxmltree::Document::parse_with_options(&rendered, options()).unwrap();
    assert_eq!(
        rendered_doc
            .descendants()
            .filter(|node| node.attribute("class") == Some("cluster"))
            .count(),
        4
    );
    for original_group in original_doc
        .descendants()
        .filter(|node| node.attribute("class") == Some("edge"))
    {
        let id = original_group.attribute("id").unwrap();
        let rendered_group = rendered_doc
            .descendants()
            .find(|node| node.attribute("id") == Some(id))
            .unwrap();
        let original_path = original_group
            .descendants()
            .find(|node| node.has_tag_name("path"))
            .unwrap()
            .attribute("d")
            .unwrap();
        let rendered_path = rendered_group
            .descendants()
            .find(|node| node.has_tag_name("path"))
            .unwrap()
            .attribute("d")
            .unwrap();
        if id.starts_with("xtask-dev-edge-") {
            assert_ne!(original_path, rendered_path);
            assert!(rendered_path.matches(" L").count() >= 8);
            assert!(
                !rendered_group
                    .descendants()
                    .any(|node| node.attribute("stroke-dasharray").is_some())
            );
        } else {
            assert_eq!(original_path, rendered_path);
        }
        for tag in ["polygon", "text"] {
            let old: Vec<_> = original_group
                .descendants()
                .filter(|node| node.has_tag_name(tag))
                .map(|node| &original[node.range()])
                .collect();
            let new: Vec<_> = rendered_group
                .descendants()
                .filter(|node| node.has_tag_name(tag))
                .map(|node| &rendered[node.range()])
                .collect();
            assert_eq!(old, new);
        }
    }
    assert_eq!(
        wave::render(&original, &graph.dev_edge_ids(&layout)).unwrap(),
        rendered
    );
}
