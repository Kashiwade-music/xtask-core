//! Role: 基準の図を読み、配置の要求に変換する。
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use xtask_layout::{Edge, Graph, Layering, Node, Region, Size};

/// 基準の一つの図。`path` の JSON の `input` を配置の要求にしたもの。
pub struct Figure {
    pub path: PathBuf,
    pub graph: Graph,
}

impl Figure {
    /// 図の群（`kivact` または `xtask-core`）。
    pub fn group(&self) -> &str {
        self.path
            .parent()
            .and_then(|dir| dir.file_name())
            .and_then(OsStr::to_str)
            .expect("a baseline figure sits in a named directory")
    }

    /// 図の名前（拡張子を除いたファイル名）。
    pub fn name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(OsStr::to_str)
            .expect("a baseline figure has a name")
    }
}

/// 基準の全件を読む。`kivact` の 121 件の後に `xtask-core` の 25 件が、それぞれファイル名の昇順で並ぶ。
pub fn load_all() -> Vec<Figure> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/layout-baseline");
    ["kivact", "xtask-core"]
        .into_iter()
        .flat_map(|group| json_files(&root.join(group)))
        .map(|path| {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            let document: Value = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
            let graph = to_graph(&document["input"]);
            Figure { path, graph }
        })
        .collect()
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    let mut paths: Vec<PathBuf> = entries
        .map(|entry| entry.expect("read a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
}

/// `input` の JSON を配置の要求にする。
fn to_graph(input: &Value) -> Graph {
    let node_values = array(input, "nodes");
    let node_index: BTreeMap<&str, usize> = node_values
        .iter()
        .enumerate()
        .map(|(index, node)| (string(&node["id"]), index))
        .collect();
    let mut regions = Vec::new();
    let mut node_region = vec![None; node_values.len()];
    collect_regions(
        array(input, "regions"),
        None,
        &node_index,
        &mut regions,
        &mut node_region,
    );
    let nodes = node_values
        .iter()
        .zip(node_region)
        .map(|(node, region)| Node {
            size: size(node, "width", "height"),
            region,
        })
        .collect();
    let label_metrics = &input["label_metrics"];
    let edges = array(input, "edges")
        .iter()
        .map(|edge| Edge {
            from: node_index[string(&edge["from"])],
            to: node_index[string(&edge["to"])],
            label: label_metrics
                .get(string(&edge["id"]))
                .map(|metrics| size(metrics, "width", "height")),
        })
        .collect();
    let layering = layering(input, &node_index, node_values.len());
    Graph {
        nodes,
        regions,
        edges,
        layering,
    }
}

/// region の木を前順にたどって index を振り、node の所属の region を埋める。
fn collect_regions(
    list: &[Value],
    parent: Option<usize>,
    node_index: &BTreeMap<&str, usize>,
    regions: &mut Vec<Region>,
    node_region: &mut [Option<usize>],
) {
    for region in list {
        let index = regions.len();
        regions.push(Region {
            parent,
            header: size(region, "label_width", "label_height"),
        });
        for id in array(region, "nodes") {
            node_region[node_index[string(id)]] = Some(index);
        }
        collect_regions(
            array(region, "children"),
            Some(index),
            node_index,
            regions,
            node_region,
        );
    }
}

/// `ranks` が空なら最適化し、空でなければ node の属する rank の番号（左から 0）で固定する。
fn layering(input: &Value, node_index: &BTreeMap<&str, usize>, node_count: usize) -> Layering {
    let ranks = array(input, "ranks");
    if ranks.is_empty() {
        return Layering::Optimize;
    }
    let mut columns: Vec<Option<u32>> = vec![None; node_count];
    for (rank, members) in ranks.iter().enumerate() {
        for id in members.as_array().expect("a baseline rank is an array") {
            columns[node_index[string(id)]] = Some(rank as u32);
        }
    }
    Layering::Fixed(
        columns
            .into_iter()
            .map(|column| column.expect("every node is in a rank"))
            .collect(),
    )
}

/// JSON の大きさ。ceil して整数にする。
fn size(value: &Value, width: &str, height: &str) -> Size {
    Size {
        width: number(value, width).ceil() as i64,
        height: number(value, height).ceil() as i64,
    }
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("baseline `{key}` is not an array"))
        .as_slice()
}

fn number(value: &Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("baseline `{key}` is not a number"))
}

fn string(value: &Value) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("a baseline id is not a string"))
}
