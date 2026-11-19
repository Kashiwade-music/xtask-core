//! Role: xtask-render の描画の規則の単体試験（角の丸め、跳び、矢印、近傍、題の幅、座標）。
use xtask_archive::style::{TITLE_FONT_SIZE, advance};
use xtask_archive::{Edge, EdgeStyle, Item, Node, Point as ScenePoint, Rect, Scene};

use crate::{Cmd, P, bridge, item_rect, neighbors, picture, route};

fn scene_item(name: &str) -> Item {
    Item {
        name: name.into(),
        role: None,
        open: None,
    }
}

fn scene_node(item: u32, x: i32, y: i32, width: i32, height: i32) -> Node {
    Node {
        item,
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        lines: vec!["node".into()],
    }
}

fn scene_edge(from: u32, to: u32, points: &[(i32, i32)]) -> Edge {
    Edge {
        from,
        to,
        style: EdgeStyle::Normal,
        points: points.iter().map(|&(x, y)| ScenePoint { x, y }).collect(),
        label: None,
    }
}

/// node は item と同じ数だけ置く（node i は item i）。
fn scene(nodes: Vec<Node>, edges: Vec<Edge>, width: i32, height: i32) -> Scene {
    let items = (0..nodes.len())
        .map(|index| scene_item(&format!("n{index}")))
        .collect();
    Scene {
        width,
        height,
        items,
        nodes,
        regions: Vec::new(),
        edges,
    }
}

/// 十字に交わる水平の edge（0）と垂直の edge（1）。edge の順を変えられる。
fn crossing_scene(horizontal_first: bool) -> Scene {
    let nodes = vec![
        scene_node(0, -10, 40, 10, 20),
        scene_node(1, 100, 40, 10, 20),
        scene_node(2, 45, -20, 10, 20),
        scene_node(3, 45, 100, 10, 20),
    ];
    let horizontal = scene_edge(0, 1, &[(0, 50), (100, 50)]);
    let vertical = scene_edge(2, 3, &[(50, 0), (50, 100)]);
    let edges = if horizontal_first {
        vec![horizontal, vertical]
    } else {
        vec![vertical, horizontal]
    };
    scene(nodes, edges, 200, 200)
}

#[test]
fn corner_is_rounded() {
    let points = [
        P { x: 0.0, y: 0.0 },
        P { x: 10.0, y: 0.0 },
        P { x: 10.0, y: 10.0 },
    ];
    let commands = route::path(&points);
    let kinds: Vec<&str> = commands
        .iter()
        .map(|command| match command {
            Cmd::Move(_) => "move",
            Cmd::Line(_) => "line",
            Cmd::Quad { .. } => "quad",
        })
        .collect();
    assert_eq!(kinds, ["move", "line", "quad", "line"]);
    assert_eq!(
        commands[2],
        Cmd::Quad {
            control: P { x: 10.0, y: 0.0 },
            to: P { x: 10.0, y: 4.0 },
        }
    );
}

#[test]
fn crossing_adds_bridge() {
    let picture = picture("cross", &crossing_scene(true));
    assert_eq!(picture.bridges.len(), 1);
    // 跳ぶのは水平の線を持つ edge（0）。
    assert_eq!(picture.bridges[0].edge, 0);
}

#[test]
fn bridge_order_does_not_depend_on_edge_order() {
    let forward = picture("cross", &crossing_scene(true));
    let reversed = picture("cross", &crossing_scene(false));
    assert_eq!(forward.bridges.len(), 1);
    assert_eq!(reversed.bridges.len(), 1);
    assert_eq!(forward.bridges[0].center, reversed.bridges[0].center);
}

#[test]
fn close_crossings_are_merged() {
    // 水平の線分 (0, 50)→(60, 50) を、x = 20、22、40 の垂直の線分が横切る。
    // 点は Picture の座標のまま bridge に渡す。
    let edges = vec![
        vec![P { x: 0.0, y: 50.0 }, P { x: 60.0, y: 50.0 }],
        vec![P { x: 20.0, y: 0.0 }, P { x: 20.0, y: 100.0 }],
        vec![P { x: 22.0, y: 0.0 }, P { x: 22.0, y: 100.0 }],
        vec![P { x: 40.0, y: 0.0 }, P { x: 40.0, y: 100.0 }],
    ];
    let xs: Vec<f64> = bridge::find(&edges)
        .iter()
        .filter(|bridge| bridge.edge == 0)
        .map(|bridge| bridge.center.x)
        .collect();
    assert_eq!(xs, vec![20.0, 40.0]);
}

#[test]
fn arrow_points_right() {
    let edge = scene(
        vec![scene_node(0, 0, 0, 10, 10), scene_node(1, 30, 10, 10, 10)],
        vec![scene_edge(0, 1, &[(0, 0), (10, 0), (10, 10), (30, 10)])],
        200,
        200,
    );
    let picture = picture("arrow", &edge);
    let arrow = picture.edges[0].arrow;
    let tip = picture.edges[0].path.last().cloned();
    assert_eq!(tip, Some(Cmd::Line(arrow[0])));
    assert!(arrow[1].x < arrow[0].x);
    assert!(arrow[2].x < arrow[0].x);
}

#[test]
fn neighbors_include_both_ends() {
    let scene = scene(
        vec![scene_node(0, 0, 0, 10, 10), scene_node(1, 50, 0, 10, 10)],
        vec![scene_edge(0, 1, &[(10, 5), (50, 5)])],
        100,
        100,
    );
    let highlight = neighbors(&scene, 0);
    assert_eq!(highlight.items.into_iter().collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(highlight.edges.into_iter().collect::<Vec<_>>(), vec![0]);
}

#[test]
fn title_widens_picture() {
    let title = "x".repeat(40);
    let scene = scene(Vec::new(), Vec::new(), 10, 10);
    let picture = picture(&title, &scene);
    let title_width: f64 = title
        .chars()
        .map(|character| advance(character, TITLE_FONT_SIZE))
        .sum();
    assert!(picture.width >= title_width + 32.0);
}

#[test]
fn item_rect_uses_picture_coordinates() {
    let scene = scene(vec![scene_node(0, 0, 0, 10, 10)], Vec::new(), 100, 100);
    let picture = picture("rect", &scene);
    let rect = item_rect(&picture, 0).expect("node 0 has a rectangle");
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (16.0, 42.0, 10.0, 10.0)
    );
}
