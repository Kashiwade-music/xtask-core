//! Role: 配置要求の決定性、文字列保持、SVG出力と失敗時の保護を検証する。
use super::*;
use std::{ffi::OsStr, fs, process::Command};

fn example() -> Graph {
    Graph {
        title: "依存関係".into(),
        nodes: vec![
            Node {
                id: "b".into(),
                label: "second".into(),
            },
            Node {
                id: "a".into(),
                label: "first".into(),
            },
        ],
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
        ],
    }
}

#[test]
fn requests_are_deterministic_and_preserve_unicode() {
    let graph = example();
    let mut reordered = graph.clone();
    reordered.nodes.reverse();
    reordered.edges.reverse();
    let request = graph.layout_request(&Layout::default()).unwrap();
    assert_eq!(
        request,
        reordered.layout_request(&Layout::default()).unwrap()
    );
    assert_eq!(request.title, "依存関係");
    assert_eq!(
        request
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        request
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str()))
            .collect::<Vec<_>>(),
        [("a", "b"), ("b", "a")]
    );
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        serde_json::to_string(&reordered.layout_request(&Layout::default()).unwrap()).unwrap()
    );
}

#[test]
fn arbitrary_labels_and_ids_survive_request_serialization() {
    let value = "日本語\"; evil -> node; \\n\n\r\t";
    let graph = Graph {
        title: value.into(),
        nodes: vec![Node {
            id: value.into(),
            label: value.into(),
        }],
        edges: vec![Edge {
            from: value.into(),
            to: value.into(),
            label: Some(value.into()),
        }],
    };
    let request = graph.layout_request(&Layout::default()).unwrap();
    assert_eq!(request.title, value);
    assert_eq!(request.nodes[0].id, value);
    assert_eq!(request.nodes[0].label, value);
    assert_eq!(request.edges[0].from, value);
    assert_eq!(request.edges[0].to, value);
    assert_eq!(request.edges[0].label.as_deref(), Some(value));
    let encoded = serde_json::to_string(&request).unwrap();
    let decoded: xtask_layout::LayoutRequest = serde_json::from_str(&encoded).unwrap();
    assert_eq!(request, decoded);
}

#[test]
fn rejects_duplicate_and_missing_nodes() {
    let mut graph = example();
    graph.nodes.push(graph.nodes[0].clone());
    assert!(
        graph
            .layout_request(&Layout::default())
            .unwrap_err()
            .contains("duplicate")
    );
    graph.nodes.pop();
    graph.edges[0].to = "missing".into();
    assert!(
        graph
            .layout_request(&Layout::default())
            .unwrap_err()
            .contains("missing")
    );
    graph.edges[0].to = "a".into();
    graph.edges[0].from = "missing".into();
    assert!(
        graph
            .layout_request(&Layout::default())
            .unwrap_err()
            .contains("missing")
    );
}

#[test]
fn unavailable_node_preserves_existing_svg_and_creates_no_extra_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let svg_path = directory.path().join("old.svg");
    fs::write(&svg_path, "old SVG").unwrap();
    let error = write_with_node(
        &example(),
        &svg_path,
        directory.path().join("missing-node-executable").as_os_str(),
    )
    .unwrap_err();
    assert!(error.contains("XTASK_NODE"));
    assert_eq!(fs::read_to_string(svg_path).unwrap(), "old SVG");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn malformed_graph_does_not_write_files() {
    let directory = tempfile::tempdir().unwrap();
    let mut graph = example();
    graph.nodes.clear();
    assert!(
        write_with_node(
            &graph,
            &directory.path().join("nodes.svg"),
            OsStr::new("node"),
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn injected_node_checks_arguments_exit_json_and_scene_validity() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fake_node.rs");
    fs::write(&source, r#"
        use std::io::Read;
        fn main() {
            let args: Vec<_> = std::env::args_os().collect();
            assert_eq!(args.len(), 3);
            assert!(std::fs::read_to_string(&args[1]).unwrap().contains("elk"));
            assert!(std::fs::metadata(&args[2]).unwrap().len() > 0);
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            assert!(input.contains("\"nodes\"") && input.contains("\"direction\""));
            if input.contains("FAIL") {
                eprintln!("fixture ELK process error");
                std::process::exit(7);
            }
            if input.contains("EMPTY") { return; }
            if input.contains("INVALID") { println!("not json"); return; }
            println!("{{\"title\":\"INCOMPLETE\",\"width\":100,\"height\":100,\"nodes\":[],\"regions\":[],\"edges\":[]}}");
        }
    "#).unwrap();
    let binary = directory
        .path()
        .join(format!("fake-node{}", std::env::consts::EXE_SUFFIX));
    let compilation = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    let svg_path = directory.path().join("日本語 space/nodes.svg");
    fs::create_dir_all(svg_path.parent().unwrap()).unwrap();
    fs::write(&svg_path, "previous SVG").unwrap();
    for (title, expected) in [
        ("FAIL", "fixture ELK process error"),
        ("EMPTY", "empty"),
        ("INVALID", "JSON"),
        ("INCOMPLETE", "scene"),
    ] {
        let mut graph = example();
        graph.title = title.into();
        let error = write_with_node(&graph, &svg_path, binary.as_os_str()).unwrap_err();
        assert!(
            error.to_lowercase().contains(&expected.to_lowercase()),
            "{title}: {error}"
        );
        assert_eq!(fs::read_to_string(&svg_path).unwrap(), "previous SVG");
        assert_eq!(fs::read_dir(svg_path.parent().unwrap()).unwrap().count(), 1);
    }
}

#[test]
fn real_elk_when_node_is_installed() {
    let binary = std::env::var_os("XTASK_NODE").unwrap_or_else(|| "node".into());
    if !Command::new(&binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("Node not available; optional ELK integration check skipped");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let svg_path = directory.path().join("nodes.svg");
    write_with_node(&example(), &svg_path, &binary).unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    let svg = fs::read_to_string(svg_path).unwrap();
    let document = roxmltree::Document::parse(&svg).unwrap();
    assert_eq!(
        document.root_element().attribute("data-layout-engine"),
        Some("ELK")
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.attribute("class") == Some("node"))
            .count(),
        2
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.attribute("class") == Some("edge"))
            .count(),
        2
    );
}
