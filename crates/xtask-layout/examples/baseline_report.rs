//! Role: 基準の全件の指標を ELK の結果と比べる表を出力する。
#[path = "../tests/common/baseline.rs"]
mod baseline;

use baseline::Figure;
use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::Instant;
use xtask_layout::{Layout, Metrics, layout, metrics};

const WRITE: &str = "writing to a String cannot fail";

/// ELK の結果のうち、比較に使うもの。
struct Elk {
    width: f64,
    height: f64,
    routes: Vec<Vec<(f64, f64)>>,
}

/// 一つの図の比較の行。
struct Row {
    group: String,
    name: String,
    nodes: usize,
    edges: usize,
    elk: Metrics,
    ours: Metrics,
    elk_area: f64,
    ours_area: f64,
    millis: f64,
}

/// 行の合計。
#[derive(Default)]
struct Totals {
    nodes: usize,
    edges: usize,
    elk_crossings: usize,
    ours_crossings: usize,
    elk_bends: usize,
    ours_bends: usize,
    elk_area: f64,
    ours_area: f64,
    millis: f64,
}

impl Totals {
    fn add(&mut self, row: &Row) {
        self.nodes += row.nodes;
        self.edges += row.edges;
        self.elk_crossings += row.elk.crossings;
        self.ours_crossings += row.ours.crossings;
        self.elk_bends += row.elk.bends;
        self.ours_bends += row.ours.bends;
        self.elk_area += row.elk_area;
        self.ours_area += row.ours_area;
        self.millis += row.millis;
    }
}

fn totals<'a>(rows: impl Iterator<Item = &'a Row>) -> Totals {
    let mut total = Totals::default();
    for row in rows {
        total.add(row);
    }
    total
}

fn main() {
    let figures = baseline::load_all();
    for figure in &figures {
        // 暖機。測らない。
        lay_out(figure);
    }
    let rows: Vec<Row> = figures.iter().map(measure).collect();
    let report = render(&rows);
    print!("{report}");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&dir).expect("create the target directory");
    fs::write(dir.join("layout-baseline-report.md"), &report).expect("write the report");
}

/// `layout` だけを測る。
fn measure(figure: &Figure) -> Row {
    let started = Instant::now();
    let result = lay_out(figure);
    let millis = started.elapsed().as_secs_f64() * 1000.0;
    let scene = read_elk(&figure.path);
    let ours_routes: Vec<Vec<(f64, f64)>> = result
        .edges
        .iter()
        .map(|route| {
            route
                .points
                .iter()
                .map(|point| (point.x as f64, point.y as f64))
                .collect()
        })
        .collect();
    let (ours_width, ours_height) = (result.size.width as f64, result.size.height as f64);
    Row {
        group: figure.group().to_string(),
        name: figure.name().to_string(),
        nodes: figure.graph.nodes.len(),
        edges: figure.graph.edges.len(),
        elk: metrics(scene.width, scene.height, &scene.routes),
        ours: metrics(ours_width, ours_height, &ours_routes),
        elk_area: scene.width * scene.height,
        ours_area: ours_width * ours_height,
        millis,
    }
}

fn lay_out(figure: &Figure) -> Layout {
    layout(&figure.graph)
        .unwrap_or_else(|error| panic!("{}/{}: {error}", figure.group(), figure.name()))
}

/// 基準の JSON の `scene`（ELK の結果）を読む。
fn read_elk(path: &Path) -> Elk {
    let text = fs::read_to_string(path).expect("read a baseline figure");
    let document: Value = serde_json::from_str(&text).expect("parse a baseline figure");
    let scene = &document["scene"];
    Elk {
        width: number(scene, "width"),
        height: number(scene, "height"),
        routes: scene["edges"]
            .as_array()
            .expect("scene edges are an array")
            .iter()
            .map(|edge| points(&edge["points"]))
            .collect(),
    }
}

fn points(value: &Value) -> Vec<(f64, f64)> {
    value
        .as_array()
        .expect("edge points are an array")
        .iter()
        .map(|point| (number(point, "x"), number(point, "y")))
        .collect()
}

fn number(value: &Value, key: &str) -> f64 {
    value[key].as_f64().expect("a scene number")
}

/// 本実装 ÷ ELK。ELK の和が 0 のとき、本実装も 0 なら 1.00、そうでなければ inf。
fn ratio(ours: f64, elk: f64) -> String {
    if elk != 0.0 {
        format!("{:.2}", ours / elk)
    } else if ours == 0.0 {
        "1.00".to_string()
    } else {
        "inf".to_string()
    }
}

fn render(rows: &[Row]) -> String {
    let mut out = String::new();
    out.push_str(
        "| group | name | nodes | edges | crossings (ELK / 本実装) | bends (ELK / 本実装) | area (ELK / 本実装) | 時間 (ms) |\n",
    );
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
    for row in rows {
        writeln!(
            out,
            "| {} | {} | {} | {} | {} / {} | {} / {} | {:.0} / {:.0} | {:.3} |",
            row.group,
            row.name,
            row.nodes,
            row.edges,
            row.elk.crossings,
            row.ours.crossings,
            row.elk.bends,
            row.ours.bends,
            row.elk_area,
            row.ours_area,
            row.millis
        )
        .expect(WRITE);
    }
    let all = totals(rows.iter());
    let kivact = totals(rows.iter().filter(|row| row.group == "kivact"));
    let kivact_count = rows.iter().filter(|row| row.group == "kivact").count();
    writeln!(
        out,
        "| 合計（{} 件） | | {} | {} | {} / {} | {} / {} | {:.0} / {:.0} | {:.3} |",
        rows.len(),
        all.nodes,
        all.edges,
        all.elk_crossings,
        all.ours_crossings,
        all.elk_bends,
        all.ours_bends,
        all.elk_area,
        all.ours_area,
        all.millis
    )
    .expect(WRITE);
    writeln!(
        out,
        "| 比（本実装 ÷ ELK）交差 / 折れ / 面積 | | | | 交差 {} | 折れ {} | 面積 {} | |",
        ratio(all.ours_crossings as f64, all.elk_crossings as f64),
        ratio(all.ours_bends as f64, all.elk_bends as f64),
        ratio(all.ours_area, all.elk_area),
    )
    .expect(WRITE);
    writeln!(
        out,
        "| kivact {} 件の時間の和 | | | | | | | {:.3} s |",
        kivact_count,
        kivact.millis / 1000.0
    )
    .expect(WRITE);
    out
}
