//! Role: viewer の画面の状態（文書・読込み・表示中の図・視点・注目）を明示の型で定義する。
use kivact::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;
use xtask_archive::DiagramKind;

/// 開いているファイルの状態。`attempted` は、最後に読もうとした時のファイルの印（読めなかった時を含む）。
#[derive(Clone)]
pub enum Document {
    Empty,
    Loaded {
        loaded: Rc<Loaded>,
        attempted: Option<Stamp>,
    },
    Failed {
        path: PathBuf,
        message: String,
        attempted: Option<Stamp>,
    },
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (
                Self::Loaded {
                    loaded: a,
                    attempted: x,
                },
                Self::Loaded {
                    loaded: b,
                    attempted: y,
                },
            ) => Rc::ptr_eq(a, b) && x == y,
            (
                Self::Failed {
                    path: a,
                    message: m,
                    attempted: x,
                },
                Self::Failed {
                    path: b,
                    message: n,
                    attempted: y,
                },
            ) => a == b && m == n && x == y,
            _ => false,
        }
    }
}

/// 読み込んだファイル。作った後は変えない（snapshot）。`pictures` と `kinds` は `document::load` だけが作る派生の値。
pub struct Loaded {
    pub path: PathBuf,
    pub archive: xtask_archive::Archive,
    /// diagrams と同じ順。Drawn のものだけ Some。
    pub pictures: Vec<Option<xtask_render::Picture>>,
    /// 図の kind から index への索引。
    pub kinds: BTreeMap<DiagramKind, usize>,
}

/// 変更の検出に使うファイルの印。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub modified: SystemTime,
    pub len: u64,
}

/// 読込みの理由。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Open,
    Reload,
}

/// 走っている読込みの要求。表示（Document）とは別の事実。新しい要求は古い要求の結果を捨てさせる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Idle,
    Running {
        id: u64,
        path: PathBuf,
        reason: Reason,
    },
}

/// 表示中の図・履歴・固定の強調。`navigate` と `document::apply` だけが作る。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nav {
    pub diagram: DiagramKind,
    pub back: Vec<DiagramKind>,
    pub pinned: Option<u32>,
}

impl Nav {
    /// 開いた直後：crate 図、空の履歴、固定なし。
    pub fn start() -> Self {
        Self {
            diagram: DiagramKind::Crates,
            back: Vec::new(),
            pinned: None,
        }
    }
}

/// 視点。Fit は canvas の大きさに合わせて描画のたびに求める。
/// Manual では、画面の点 = Picture の点 * scale + offset。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Camera {
    Fit,
    Manual {
        offset_x: f64,
        offset_y: f64,
        scale: f64,
    },
}

/// 実際の視点（Fit を解いた値）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale: f64,
}

/// 押した後の pointer の扱い。canvas の handler だけが読み書きする（追跡しない Var）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Idle,
    /// 押したが、まだ 3 px 動いていない。`target` は押した region の item。
    Pressed {
        start: (f64, f64),
        view: View,
        target: Option<u32>,
    },
    /// 3 px 以上動いた。視点を動かしている。
    Dragging {
        start: (f64, f64),
        view: View,
    },
}

/// 画面の状態の handle（Copy）。
#[derive(Clone, Copy)]
pub struct AppState {
    pub document: Signal<Document>,
    pub request: Signal<Request>,
    /// 最後に出した要求の id。
    pub last_request: Var<u64>,
    pub nav: Signal<Nav>,
    pub camera: Signal<Camera>,
    /// pointer が指している item。固定（`Nav.pinned`）とは独立。
    pub hover: Signal<Option<u32>>,
    /// tooltip を出す window 座標。
    pub pointer: Signal<Point>,
    pub search: Signal<String>,
    /// 読直しの失敗などの一時の通知。
    pub note: Signal<Option<String>>,
    /// 最後に描いた canvas の大きさ。描画の closure が書き、handler が読む（追跡しない）。
    pub viewport: Var<(f64, f64)>,
    pub gesture: Var<Gesture>,
}

impl AppState {
    /// setup の中で呼ぶ。
    pub fn new() -> Self {
        Self {
            document: signal(Document::Empty),
            request: signal(Request::Idle),
            last_request: var(0),
            nav: signal(Nav::start()),
            camera: signal(Camera::Fit),
            hover: signal(None),
            pointer: signal(Point::new(0.0, 0.0)),
            search: signal(String::new()),
            note: signal(None),
            viewport: var((960.0, 640.0)),
            gesture: var(Gesture::Idle),
        }
    }

    /// 表示中の図の Loaded と index。文書が Loaded でなければ None。
    pub fn current(&self) -> Option<(Rc<Loaded>, usize)> {
        let Document::Loaded { loaded, .. } = self.document.get() else {
            return None;
        };
        let index = self
            .nav
            .with(|nav| loaded.kinds.get(&nav.diagram).copied())
            .unwrap_or(0);
        Some((loaded, index))
    }
}
