//! Role: 図のファイルの符号化・復号・検証と、文字の折り返しの試験を定義する。

use crate::codec::{wrap, write_body, write_unchecked};
use crate::style::measure;
use crate::{
    Archive, Content, DecodeError, Diagram, DiagramKind, Edge, EdgeStyle, Item, Label, Mode, Node,
    Point, Rect, Region, Scene, decode, encode, validate,
};

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn point(x: i32, y: i32) -> Point {
    Point { x, y }
}

fn module_kind(package: &str, target: &str, kind: &str, mode: Mode) -> DiagramKind {
    DiagramKind::Modules {
        package: package.into(),
        target: target.into(),
        kind: kind.into(),
        mode,
    }
}

/// 試験の Archive。図は crate・module・失敗・未解析の四つ。
fn sample() -> Archive {
    let crates = Diagram {
        kind: DiagramKind::Crates,
        title: "Workspace crate dependencies (consumer -> provider)".into(),
        content: Content::Drawn(Scene {
            width: 240,
            height: 40,
            items: vec![
                Item {
                    name: "a".into(),
                    role: Some("front".into()),
                    open: Some(1),
                },
                Item {
                    name: "b".into(),
                    role: None,
                    open: None,
                },
            ],
            nodes: vec![
                Node {
                    item: 0,
                    rect: rect(0, 0, 100, 40),
                    lines: vec!["a".into(), "front".into()],
                },
                Node {
                    item: 1,
                    rect: rect(140, 0, 100, 40),
                    lines: vec!["b".into(), "[役割未記載]".into()],
                },
            ],
            regions: vec![],
            edges: vec![Edge {
                from: 0,
                to: 1,
                style: EdgeStyle::Normal,
                points: vec![point(100, 20), point(140, 20)],
                label: Some(Label {
                    rect: rect(100, 2, 40, 16),
                    lines: vec!["target".into()],
                }),
            }],
        }),
    };
    let module = Diagram {
        kind: module_kind("a", "a", "lib", Mode::Normal),
        title: "a::a (lib, normal): module dependencies".into(),
        content: Content::Drawn(Scene {
            width: 260,
            height: 140,
            items: vec![
                Item {
                    name: "crate".into(),
                    role: Some("root".into()),
                    open: None,
                },
                Item {
                    name: "crate::x".into(),
                    role: None,
                    open: None,
                },
            ],
            nodes: vec![
                Node {
                    item: 0,
                    rect: rect(10, 40, 80, 30),
                    lines: vec!["(self)".into()],
                },
                Node {
                    item: 1,
                    rect: rect(140, 60, 80, 30),
                    lines: vec!["(self)".into()],
                },
            ],
            regions: vec![
                Region {
                    item: 0,
                    parent: None,
                    rect: rect(0, 0, 260, 140),
                    lines: vec!["crate".into(), "root".into()],
                },
                Region {
                    item: 1,
                    parent: Some(0),
                    rect: rect(120, 30, 120, 90),
                    lines: vec!["crate::x".into(), "[役割未記載]".into()],
                },
            ],
            edges: vec![Edge {
                from: 0,
                to: 1,
                style: EdgeStyle::Normal,
                points: vec![point(90, 55), point(140, 55)],
                label: None,
            }],
        }),
    };
    let failed = Diagram {
        kind: module_kind("a", "a", "lib", Mode::Test),
        title: "a::a (lib, test): module dependencies".into(),
        content: Content::Failed("cannot analyze".into()),
    };
    let skipped = Diagram {
        kind: module_kind("b", "b", "bin", Mode::Normal),
        title: "b::b (bin, normal): module dependencies".into(),
        content: Content::Skipped("required-features are disabled".into()),
    };
    Archive {
        root_name: "repo".into(),
        diagrams: vec![crates, module, failed, skipped],
        notices: vec!["sample notice".into()],
    }
}

/// 試験の本体の文字列の符号（u32 の長さと UTF-8）。
fn text(value: &str) -> Vec<u8> {
    let mut bytes = (value.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

/// 検証に通らない値を作るための helper。
fn broken(change: impl FnOnce(&mut Archive)) -> Archive {
    let mut archive = sample();
    change(&mut archive);
    archive
}

fn scene_mut(archive: &mut Archive, index: usize) -> &mut Scene {
    match &mut archive.diagrams[index].content {
        Content::Drawn(scene) => scene,
        _ => panic!("diagram {index} is not drawn"),
    }
}

#[test]
fn round_trip() {
    let archive = sample();
    let bytes = encode(&archive).unwrap();
    assert_eq!(decode(&bytes).unwrap(), archive);
}

#[test]
fn encode_is_deterministic() {
    let archive = sample();
    assert_eq!(encode(&archive).unwrap(), encode(&archive).unwrap());
}

#[test]
fn rejects_other_files() {
    assert_eq!(decode(b"hello"), Err(DecodeError::NotArchive));
    let mut bytes = encode(&sample()).unwrap();
    bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(decode(&bytes), Err(DecodeError::Version { found: 2 }));
}

#[test]
fn rejects_bad_reference() {
    let bad = broken(|archive| {
        let scene = scene_mut(archive, 0);
        scene.edges[0].to = scene.nodes.len() as u32;
    });
    assert!(encode(&bad).is_err());
    let body = write_body(&bad).unwrap();
    assert!(matches!(decode(&wrap(&body)), Err(DecodeError::Corrupt(_))));
}

#[test]
fn truncation_never_panics() {
    let bytes = encode(&sample()).unwrap();
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length]).is_err(), "length {length} decoded");
    }
}

#[test]
fn trailing_garbage_rejected() {
    let mut bytes = encode(&sample()).unwrap();
    bytes.extend_from_slice(b"GARBAGE");
    assert_eq!(
        decode(&bytes),
        Err(DecodeError::Corrupt("bad compression".into()))
    );
}

#[test]
fn huge_count_rejected() {
    let body = [text(""), u32::MAX.to_le_bytes().to_vec()].concat();
    assert_eq!(
        decode(&wrap(&body)),
        Err(DecodeError::Corrupt("length out of range".into()))
    );
}

#[test]
fn allocation_budget_limits_decode() {
    let mut body = text("");
    body.extend_from_slice(&1000u32.to_le_bytes());
    for _ in 0..1000 {
        body.push(0);
        body.extend(text(""));
        body.push(1);
        body.extend(text(""));
    }
    body.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode(&wrap(&body)),
        Err(DecodeError::Corrupt("allocation budget exceeded".into()))
    );
}

#[test]
fn invalid_utf8_rejected() {
    let body = [1u32.to_le_bytes().to_vec(), vec![0xff]].concat();
    assert_eq!(
        decode(&wrap(&body)),
        Err(DecodeError::Corrupt("invalid UTF-8".into()))
    );
}

#[test]
fn unknown_tag_rejected() {
    let mut body = text("");
    body.extend_from_slice(&1u32.to_le_bytes());
    body.push(0);
    body.extend(text(""));
    body.push(9);
    assert_eq!(
        decode(&wrap(&body)),
        Err(DecodeError::Corrupt("unknown tag 9".into()))
    );
}

#[test]
fn validate_rules() {
    assert!(validate(&sample()).is_ok());
    let broken_cases = [
        broken(|archive| archive.diagrams[0].kind = module_kind("a", "a", "lib", Mode::Normal)),
        broken(|archive| {
            let kind = archive.diagrams[1].kind.clone();
            archive.diagrams[2].kind = kind;
        }),
        broken(|archive| scene_mut(archive, 0).edges[0].points[1] = point(140, 30)),
        broken(|archive| {
            scene_mut(archive, 0).edges[0].points = vec![point(140, 20), point(100, 20)];
        }),
        broken(|archive| {
            scene_mut(archive, 1).regions.pop();
        }),
        broken(|archive| scene_mut(archive, 0).items[0].open = Some(0)),
    ];
    for case in &broken_cases {
        assert!(
            validate(case).is_err(),
            "validate accepted a broken archive"
        );
    }
}

#[test]
fn measure_wraps() {
    let block = measure("abcdef", 10.0, 18.0, 20.0);
    assert_eq!(block.lines, vec!["abc".to_string(), "def".to_string()]);
    assert_eq!(block.height, 36.0);
}

/// 試験の module 図の数。予算（8 × 本体の長さ + 4096）を、符号の費用（Diagram の大きさ）が超える数。
const MODULE_COUNT: usize = 2500;

/// 一文字の名前を、index の mixed radix の桁から作る（index の昇順が、module 図の kind の昇順と一致する）。
fn letter(digit: usize) -> String {
    char::from(b'a' + digit as u8).to_string()
}

/// 最小の module 図（名前は一文字、題は空、解析しなかった図）。index の昇順に kind が厳密に増える。
fn minimal_module(index: usize) -> Diagram {
    let package = letter(index / (52 * 26));
    let target = letter((index / 52) % 26);
    let kind = letter((index / 2) % 26);
    let mode = if index % 2 == 0 {
        Mode::Normal
    } else {
        Mode::Test
    };
    Diagram {
        kind: module_kind(&package, &target, &kind, mode),
        title: String::new(),
        content: Content::Skipped(String::new()),
    }
}

#[test]
fn encode_rejects_what_decode_would_reject() {
    let mut diagrams = vec![sample().diagrams.remove(0)];
    diagrams.extend((0..MODULE_COUNT).map(minimal_module));
    let archive = Archive {
        root_name: String::new(),
        diagrams,
        notices: Vec::new(),
    };
    assert_eq!(encode(&archive), Err("archive is too large".to_string()));
    let (body, _) = write_unchecked(&archive).unwrap();
    assert_eq!(
        decode(&wrap(&body)),
        Err(DecodeError::Corrupt("allocation budget exceeded".into()))
    );
}
