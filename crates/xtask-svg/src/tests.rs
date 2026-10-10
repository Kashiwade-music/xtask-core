//! Role: SVGの階層・接続・波線・交差ジャンプと不正な形状を検証する。
use xtask_layout::{EdgeStyle, Point, Scene, SceneEdge, SceneLabel, SceneNode, SceneRegion};

use super::crossings::crossings;
use super::drawing;
use super::render;
use super::route::{self, Path, Point as GeometryPoint, Segment};

fn node(id: &str, x: f64, y: f64) -> SceneNode {
    SceneNode {
        id: id.into(),
        label: id.into(),
        lines: vec![id.into()],
        x,
        y,
        width: 40.0,
        height: 20.0,
    }
}

fn fixture() -> Scene {
    Scene {
        title: "Geometry fixture".into(),
        width: 300.0,
        height: 180.0,
        nodes: vec![
            node("h-from", 0.0, 70.0),
            node("h-to", 260.0, 70.0),
            node("v-from", 60.0, 0.0),
            node("v-to", 220.0, 140.0),
        ],
        regions: vec![],
        edges: vec![
            SceneEdge {
                id: "h".into(),
                from: "h-from".into(),
                to: "h-to".into(),
                points: vec![Point { x: 40.0, y: 80.0 }, Point { x: 260.0, y: 80.0 }],
                label: None,
                style: EdgeStyle::Normal,
            },
            SceneEdge {
                id: "v".into(),
                from: "v-from".into(),
                to: "v-to".into(),
                points: vec![
                    Point { x: 100.0, y: 10.0 },
                    Point { x: 160.0, y: 10.0 },
                    Point { x: 160.0, y: 150.0 },
                    Point { x: 220.0, y: 150.0 },
                ],
                label: None,
                style: EdgeStyle::Normal,
            },
        ],
    }
}

#[test]
fn svg_uses_native_side_ports_and_order_independent_jumps() {
    let scene = fixture();
    let output = render(&scene).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    assert_eq!(
        document.root_element().attribute("data-layout-engine"),
        Some("ELK")
    );
    let h = document
        .descendants()
        .find(|node| {
            node.attribute("data-edge-id") == Some("h") && node.attribute("class") == Some("edge")
        })
        .unwrap();
    assert_eq!(
        h.attribute("data-points"),
        Some("40.000,80.000 260.000,80.000")
    );
    assert_eq!(h.attribute("data-end-x"), Some("260.000"));
    assert!(
        h.descendants()
            .find(|node| node.attribute("class") == Some("arrowhead"))
            .unwrap()
            .attribute("points")
            .unwrap()
            .starts_with("260.000,80.000 ")
    );
    assert!(
        h.descendants()
            .find(|node| node.has_tag_name("path"))
            .unwrap()
            .attribute("d")
            .unwrap()
            .contains(" A")
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.attribute("class") == Some("bridge"))
            .count(),
        1
    );
    let mut reordered = scene;
    reordered.nodes.reverse();
    reordered.edges.reverse();
    assert_eq!(output, render(&reordered).unwrap());
}

#[test]
fn nested_regions_escape_text_and_share_wrapped_line_measurements() {
    let mut scene = fixture();
    scene.regions = vec![
        SceneRegion {
            id: "root<&\"".into(),
            parent: None,
            label: "領域<&\"".into(),
            lines: vec!["領域<&\"".into()],
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 180.0,
        },
        SceneRegion {
            id: "inner".into(),
            parent: Some("root<&\"".into()),
            label: "内側".into(),
            lines: vec!["内側".into()],
            x: 45.0,
            y: 40.0,
            width: 210.0,
            height: 135.0,
        },
    ];
    scene.nodes[0].lines = vec!["日本語".into(), "a<&\"🙂".into()];
    let output = render(&scene).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let root = document
        .descendants()
        .find(|node| {
            node.attribute("class") == Some("cluster")
                && node.attribute("data-region-id") == Some("root<&\"")
        })
        .unwrap();
    assert!(
        root.descendants()
            .any(|node| node.attribute("data-region-id") == Some("inner"))
    );
    assert!(
        document
            .descendants()
            .any(|node| node.has_tag_name("text") && node.text() == Some("a<&\"🙂"))
    );
    assert!(!output.contains("<text>normal</text>"));
}

#[test]
fn dev_has_real_waves_label_and_protected_true_arc() {
    let mut scene = fixture();
    scene.edges[0].style = EdgeStyle::Dev;
    scene.edges[0].label = Some(SceneLabel {
        text: "dev".into(),
        lines: vec!["dev".into()],
        x: 195.0,
        y: 55.0,
        width: 30.0,
        height: 18.0,
    });
    let output = render(&scene).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let edge = document
        .descendants()
        .find(|node| {
            node.attribute("class") == Some("edge") && node.attribute("data-edge-id") == Some("h")
        })
        .unwrap();
    let data = edge
        .descendants()
        .find(|node| node.has_tag_name("path"))
        .unwrap()
        .attribute("d")
        .unwrap();
    assert!(data.starts_with("M40.000,80.000"));
    assert!(data.ends_with(" L260.000,80.000"));
    assert!(data.matches(" L").count() > 100);
    assert!(data.contains(" A"));
    assert!(
        document
            .descendants()
            .any(|node| node.has_tag_name("text") && node.text() == Some("dev"))
    );
    assert!(data.contains(",78."));
    assert!(data.contains(",82."));
}

#[test]
fn renderer_rejects_invalid_scene_and_vertical_box_ports() {
    let mut scene = fixture();
    scene.edges[0].points[0] = Point { x: 20.0, y: 70.0 };
    assert!(render(&scene).unwrap_err().contains("horizontally"));
    let mut scene = fixture();
    scene.nodes[0].width = f64::NAN;
    assert!(render(&scene).is_err());
    let mut scene = fixture();
    scene.edges[0].points.insert(1, Point { x: 100.0, y: 81.0 });
    assert!(render(&scene).is_err());
    let mut scene = fixture();
    scene.regions.push(SceneRegion {
        id: "cycle".into(),
        parent: Some("cycle".into()),
        label: String::new(),
        lines: vec![],
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    });
    assert!(render(&scene).is_err());
}

#[test]
fn reversed_dependency_and_self_loop_keep_arrow_tips_on_box_sides() {
    let mut scene = fixture();
    scene.edges = vec![SceneEdge {
        id: "reverse".into(),
        from: "h-to".into(),
        to: "h-from".into(),
        points: vec![Point { x: 260.0, y: 80.0 }, Point { x: 40.0, y: 80.0 }],
        label: None,
        style: EdgeStyle::Dev,
    }];
    let output = render(&scene).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let arrow = document
        .descendants()
        .find(|node| node.attribute("class") == Some("arrowhead"))
        .unwrap();
    assert_eq!(
        arrow.attribute("points"),
        Some("40.000,80.000 48.000,76.800 48.000,83.200")
    );
    scene.nodes = vec![node("self", 100.0, 70.0)];
    scene.edges = vec![SceneEdge {
        id: "loop".into(),
        from: "self".into(),
        to: "self".into(),
        points: vec![
            Point { x: 140.0, y: 80.0 },
            Point { x: 170.0, y: 80.0 },
            Point { x: 170.0, y: 110.0 },
            Point { x: 70.0, y: 110.0 },
            Point { x: 70.0, y: 80.0 },
            Point { x: 100.0, y: 80.0 },
        ],
        label: None,
        style: EdgeStyle::Normal,
    }];
    let output = render(&scene).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let arrow = document
        .descendants()
        .find(|node| node.attribute("class") == Some("arrowhead"))
        .unwrap();
    assert!(
        arrow
            .attribute("points")
            .unwrap()
            .starts_with("100.000,80.000 ")
    );
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.attribute("class") == Some("bridge"))
            .count(),
        0
    );
}

fn geometry_path(points: &[(f64, f64)], group: usize) -> Path {
    let points: Vec<_> = points
        .iter()
        .map(|&(x, y)| GeometryPoint { x, y })
        .collect();
    route::build(&points, group, false).unwrap()
}

fn corner_path(second: bool, group: usize) -> Path {
    let points = if second {
        [
            (0.0, 6.0),
            (44.0, 6.0),
            (50.0, 6.0),
            (50.0, 0.0),
            (50.0, -40.0),
        ]
    } else {
        [
            (0.0, 0.0),
            (44.0, 0.0),
            (50.0, 0.0),
            (50.0, 6.0),
            (50.0, 40.0),
        ]
    };
    let p: Vec<_> = points
        .into_iter()
        .map(|(x, y)| GeometryPoint { x, y })
        .collect();
    Path {
        group,
        dev: false,
        segments: vec![
            Segment {
                start: p[0],
                end: p[1],
                straight: true,
                control: None,
            },
            Segment {
                start: p[1],
                end: p[3],
                straight: false,
                control: Some(p[2]),
            },
            Segment {
                start: p[3],
                end: p[4],
                straight: true,
                control: None,
            },
        ],
    }
}

#[test]
fn dense_crossings_shrink_bridges_and_preserve_point_order() {
    let paths = vec![
        geometry_path(&[(0.0, 0.0), (100.0, 0.0)], 0),
        geometry_path(&[(50.0, -40.0), (50.0, 40.0)], 1),
        geometry_path(&[(55.0, -40.0), (55.0, 40.0)], 2),
    ];
    let jumps = crossings(&paths, &[]);
    let output = drawing::draw(&paths[0], &jumps[0]).unwrap();
    assert_eq!(output.matches(" A2.000,2.000 ").count(), 2);
    assert!(output.contains("52.000,0.000 L53.000,0.000 A"));
}

#[test]
fn curve_crossings_trim_q_exactly_and_keep_dev_arc() {
    let mut paths = vec![corner_path(false, 0), corner_path(true, 1)];
    let jumps = crossings(&paths, &[]);
    assert_eq!(jumps[0].len(), 1);
    let output = drawing::draw(&paths[0], &jumps[0]).unwrap();
    assert_eq!(output.matches(" A").count(), 1);
    assert_eq!(output.matches(" Q").count(), 2);
    paths[0].dev = true;
    let output = drawing::draw(&paths[0], &jumps[0]).unwrap();
    assert!(output.starts_with("M0.000,0.000"));
    assert!(output.ends_with(" L50.000,40.000"));
    assert!(output.contains(" A"));
}

#[test]
fn both_internal_joins_and_neighboring_crossings_are_not_lost_or_duplicated() {
    let p = |x, y| GeometryPoint { x, y };
    let first = corner_path(false, 0);
    let second = Path {
        group: 1,
        dev: false,
        segments: vec![
            Segment {
                start: p(44.0, -40.0),
                end: p(44.0, 0.0),
                straight: true,
                control: None,
            },
            Segment {
                start: p(44.0, 0.0),
                end: p(50.0, 6.0),
                straight: false,
                control: Some(p(44.0, 6.0)),
            },
            Segment {
                start: p(50.0, 6.0),
                end: p(100.0, 6.0),
                straight: true,
                control: None,
            },
        ],
    };
    let paths = vec![first, second];
    let jumps = crossings(&paths, &[]);
    assert_eq!(jumps.iter().map(Vec::len).sum::<usize>(), 2);
    let output = drawing::draw(&paths[0], &jumps[0]).unwrap();
    assert_eq!(output.matches(" A").count(), 2);
    assert!(output.ends_with(" L50.000,40.000"));
}

#[test]
fn global_endpoints_and_overlapping_routes_are_not_crossings() {
    for second in [
        vec![(50.0, 0.0), (50.0, 40.0)],
        vec![(0.0, 0.0), (100.0, 0.0)],
    ] {
        let paths = vec![
            geometry_path(&[(0.0, 0.0), (100.0, 0.0)], 0),
            geometry_path(&second, 1),
        ];
        assert!(crossings(&paths, &[]).iter().all(Vec::is_empty));
    }
}

#[test]
fn rounded_route_keeps_endpoints_and_all_turns_local() {
    let path = geometry_path(&[(0.0, 0.0), (30.0, 0.0), (30.0, 40.0), (60.0, 40.0)], 0);
    assert_eq!(
        drawing::draw(&path, &[]).unwrap(),
        "M0.000,0.000 L26.000,0.000 Q30.000,0.000 30.000,4.000 L30.000,36.000 Q30.000,40.000 34.000,40.000 L60.000,40.000"
    );
}

#[test]
fn tangential_curves_and_repeated_curve_geometry_have_no_false_bridge() {
    let p = |x, y| GeometryPoint { x, y };
    let path = |start, control, end, group| Path {
        group,
        dev: false,
        segments: vec![Segment {
            start,
            end,
            straight: false,
            control: Some(control),
        }],
    };
    let first = path(p(0.0, 0.0), p(5.0, 10.0), p(10.0, 0.0), 0);
    let tangent = path(p(0.0, 10.0), p(5.0, 0.0), p(10.0, 10.0), 1);
    assert!(crossings(&[first, tangent], &[]).iter().all(Vec::is_empty));
    let first = corner_path(false, 0);
    let repeated = corner_path(false, 1);
    assert!(crossings(&[first, repeated], &[]).iter().all(Vec::is_empty));
}
