use super::*;
use crate::model::quote;

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
fn dot_is_deterministic_and_preserves_unicode() {
    let graph = example();
    let mut reordered = graph.clone();
    reordered.nodes.reverse();
    reordered.edges.reverse();
    assert_eq!(graph.dot(), reordered.dot());
    assert_eq!(
        graph.dot().unwrap(),
        concat!(
            "digraph {\n",
            "  graph [label=\"依存関係\", labelloc=\"t\", rankdir=\"TB\"];\n",
            "  node [shape=\"box\"];\n",
            "  \"a\" [label=\"first\"];\n",
            "  \"b\" [label=\"second\"];\n",
            "  \"a\" -> \"b\" [label=\"uses\"];\n",
            "  \"b\" -> \"a\";\n",
            "}\n",
        )
    );
}

#[test]
fn quotes_cannot_inject_dot_statements() {
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
    let dot = graph.dot().unwrap();
    assert_eq!(quote(value), "\"日本語\\\"; evil -> node; \\\\n\\n\\r\t\"");
    assert_eq!(dot.lines().count(), 6);
    assert!(dot.contains(&format!(
        "{} -> {} [label={}]",
        quote(value),
        quote(value),
        quote(value)
    )));
}

#[test]
fn rejects_duplicate_and_missing_nodes() {
    let mut graph = example();
    graph.nodes.push(graph.nodes[0].clone());
    assert!(graph.dot().unwrap_err().contains("duplicate"));
    graph.nodes.pop();
    graph.edges[0].to = "missing".into();
    assert!(graph.dot().unwrap_err().contains("missing"));
    graph.edges[0].to = "a".into();
    graph.edges[0].from = "missing".into();
    assert!(graph.dot().unwrap_err().contains("missing"));
}

#[test]
fn unavailable_graphviz_retains_dot_and_existing_svg() {
    let directory = tempfile::tempdir().unwrap();
    let dot_path = directory.path().join("output/nodes.dot");
    let svg_path = directory.path().join("old.svg");
    fs::write(&svg_path, "old SVG").unwrap();
    let error = write_with_dot(
        &example(),
        &dot_path,
        &svg_path,
        directory.path().join("missing-dot-executable").as_os_str(),
    )
    .unwrap_err();
    assert!(error.contains("XTASK_GRAPHVIZ_DOT"));
    assert_eq!(
        fs::read_to_string(dot_path).unwrap(),
        example().dot().unwrap()
    );
    assert_eq!(fs::read_to_string(svg_path).unwrap(), "old SVG");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn malformed_graph_does_not_write_files() {
    let directory = tempfile::tempdir().unwrap();
    let mut graph = example();
    graph.nodes.clear();
    assert!(
        write_with_dot(
            &graph,
            &directory.path().join("nodes.dot"),
            &directory.path().join("nodes.svg"),
            OsStr::new("dot"),
        )
        .is_err()
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    assert!(
        write_with_dot(
            &example(),
            &directory.path().join("same"),
            &directory.path().join("same"),
            OsStr::new("dot"),
        )
        .unwrap_err()
        .contains("different")
    );
}

#[test]
fn injected_graphviz_checks_arguments_exit_and_nonempty_output() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fake_dot.rs");
    fs::write(
        &source,
        r#"
        fn main() {
            let args: Vec<_> = std::env::args_os().collect();
            assert_eq!(args.len(), 5);
            assert_eq!(args[1], "-Tsvg");
            assert_eq!(args[3], "-o");
            let dot = std::fs::read_to_string(&args[2]).unwrap();
            if dot.contains("FAIL") {
                eprintln!("fixture render error");
                std::process::exit(7);
            }
            if dot.contains("EMPTY") { return; }
            std::fs::write(&args[4], "<svg xmlns='http://www.w3.org/2000/svg'></svg>").unwrap();
        }
    "#,
    )
    .unwrap();
    let binary = directory
        .path()
        .join(format!("fake-dot{}", std::env::consts::EXE_SUFFIX));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compilation = Command::new(rustc)
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
    let dot_path = directory.path().join("日本語 space/nodes.dot");
    let svg_path = directory.path().join("日本語 space/nodes.svg");
    write_with_dot(&example(), &dot_path, &svg_path, binary.as_os_str()).unwrap();
    let svg = fs::read_to_string(&svg_path).unwrap();
    assert!(svg.starts_with("<svg"));
    for (title, expected) in [("FAIL", "fixture render error"), ("EMPTY", "empty SVG")] {
        let mut graph = example();
        graph.title = title.into();
        let error = write_with_dot(&graph, &dot_path, &svg_path, binary.as_os_str()).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(fs::read_to_string(&svg_path).unwrap(), svg);
        assert_eq!(fs::read_to_string(&dot_path).unwrap(), graph.dot().unwrap());
    }
    assert_eq!(fs::read_dir(svg_path.parent().unwrap()).unwrap().count(), 2);
}

#[test]
fn real_graphviz_when_installed() {
    let binary = std::env::var_os("XTASK_GRAPHVIZ_DOT").unwrap_or_else(|| "dot".into());
    if !Command::new(&binary)
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        eprintln!("Graphviz not available; optional integration check skipped");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let svg_path = directory.path().join("nodes.svg");
    write_with_dot(
        &example(),
        &directory.path().join("nodes.dot"),
        &svg_path,
        &binary,
    )
    .unwrap();
    assert!(fs::read_to_string(svg_path).unwrap().contains("<svg"));
}
