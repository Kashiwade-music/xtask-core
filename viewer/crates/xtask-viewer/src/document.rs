//! Role: 図のファイルの読込み（純粋な読込み、結果を状態に写す規則、要求の受理、開く・監視）。
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use kivact::prelude::{blocking, spawn};
use xtask_archive::{Content, DiagramKind, MAX_FILE_LEN};

use crate::state::{AppState, Camera, Document, Loaded, Nav, Reason, Request, Stamp};

/// 読込みの結果。`blocking` の戻り値として UI の thread の外から返すので Send。
pub struct LoadResult {
    pub path: PathBuf,
    pub stamp: Option<Stamp>,
    pub loaded: Result<Loaded, String>,
}

/// 読込みの結果を写した後の状態（`apply` の戻り値）。
pub struct Applied {
    pub document: Document,
    pub nav: Nav,
    pub fit: bool,
    pub note: Option<String>,
}

/// ファイルの印。読めなければ io の誤り（文は呼出し側が path と合わせる）。
fn stamp_of(path: &Path) -> std::io::Result<Stamp> {
    let metadata = std::fs::metadata(path)?;
    Ok(Stamp {
        modified: metadata.modified()?,
        len: metadata.len(),
    })
}

/// 読込みの本体。大きさの確認は呼出し側が済ませている。
fn read_archive(path: &Path) -> Result<Loaded, String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let bytes = std::fs::read(path).map_err(|error| describe(&error))?;
    let archive = xtask_archive::decode(&bytes).map_err(|error| describe(&error))?;
    let pictures = archive
        .diagrams
        .iter()
        .map(|diagram| match &diagram.content {
            Content::Drawn(scene) => Some(xtask_render::picture(&diagram.title, scene)),
            Content::Failed(_) | Content::Skipped(_) => None,
        })
        .collect();
    let kinds = archive
        .diagrams
        .iter()
        .enumerate()
        .map(|(index, diagram)| (diagram.kind.clone(), index))
        .collect();
    Ok(Loaded {
        path: path.to_path_buf(),
        archive,
        pictures,
        kinds,
    })
}

/// ファイルを読む（UI の thread の外で呼ぶ）。
pub fn load(path: &Path) -> LoadResult {
    let stamp = stamp_of(path);
    let loaded = match &stamp {
        Err(error) => Err(format!("{}: {error}", path.display())),
        Ok(stamp) if stamp.len > MAX_FILE_LEN => {
            Err(format!("{}: file is too large", path.display()))
        }
        Ok(_) => read_archive(path),
    };
    LoadResult {
        path: path.to_path_buf(),
        stamp: stamp.ok(),
        loaded,
    }
}

/// 読直しの nav：図は新しい索引にあれば保ち、無ければ crate 図。履歴は残る図だけ。固定は解く。
fn reload_nav(nav: &Nav, kinds: &BTreeMap<DiagramKind, usize>) -> Nav {
    let diagram = if kinds.contains_key(&nav.diagram) {
        nav.diagram.clone()
    } else {
        DiagramKind::Crates
    };
    let back = nav
        .back
        .iter()
        .filter(|kind| kinds.contains_key(*kind))
        .cloned()
        .collect();
    Nav {
        diagram,
        back,
        pinned: None,
    }
}

/// 読込みの結果を状態に写す。読込みの結果を状態に写す規則の唯一の場所。
pub fn apply(previous: &Document, nav: &Nav, reason: Reason, result: LoadResult) -> Applied {
    let LoadResult {
        path,
        stamp,
        loaded,
    } = result;
    match loaded {
        Ok(loaded) => {
            let (next, fit) = match reason {
                Reason::Open => (Nav::start(), true),
                Reason::Reload => {
                    let next = reload_nav(nav, &loaded.kinds);
                    let fit = next.diagram != nav.diagram;
                    (next, fit)
                }
            };
            Applied {
                document: Document::Loaded {
                    loaded: Rc::new(loaded),
                    attempted: stamp,
                },
                nav: next,
                fit,
                note: None,
            }
        }
        Err(message) => match previous {
            Document::Loaded { loaded, attempted } => {
                let document = match reason {
                    Reason::Open => previous.clone(),
                    Reason::Reload => Document::Loaded {
                        loaded: loaded.clone(),
                        attempted: stamp.or(*attempted),
                    },
                };
                Applied {
                    document,
                    nav: nav.clone(),
                    fit: false,
                    note: Some(message),
                }
            }
            Document::Empty | Document::Failed { .. } => Applied {
                document: Document::Failed {
                    path,
                    message,
                    attempted: stamp,
                },
                nav: Nav::start(),
                fit: true,
                note: None,
            },
        },
    }
}

/// 走っている要求の id と一致するか（古い完了を捨てる）。
pub fn accepts(request: &Request, id: u64) -> bool {
    matches!(request, Request::Running { id: running, .. } if *running == id)
}

/// 読込みを始める。新しい要求は古い要求の結果を捨てさせる。
pub fn open(state: AppState, path: PathBuf, reason: Reason) {
    let id = state.last_request.get() + 1;
    state.last_request.set(id);
    state.request.set(Request::Running {
        id,
        path: path.clone(),
        reason,
    });
    spawn(async move {
        let result = blocking(move || load(&path)).await;
        if !accepts(&state.request.get_untracked(), id) {
            return;
        }
        state.request.set(Request::Idle);
        let applied = apply(
            &state.document.get_untracked(),
            &state.nav.get_untracked(),
            reason,
            result,
        );
        state.document.set(applied.document);
        state.nav.set(applied.nav);
        if applied.fit {
            state.camera.set(Camera::Fit);
        }
        state.hover.set(None);
        state.note.set(applied.note);
    });
}

/// 500 ms ごとに印を調べ、変われば読み直す。二回続けて印を取れなければ通知する。
pub fn watch(state: AppState) {
    spawn(async move {
        watch_loop(state).await;
    });
}

async fn watch_loop(state: AppState) {
    let mut failures: u32 = 0;
    loop {
        kivact::task::sleep(Duration::from_millis(500)).await;
        if matches!(state.request.get_untracked(), Request::Running { .. }) {
            continue;
        }
        let (path, attempted) = match state.document.get_untracked() {
            Document::Empty => continue,
            Document::Loaded { loaded, attempted } => (loaded.path.clone(), attempted),
            Document::Failed {
                path, attempted, ..
            } => (path, attempted),
        };
        let probe = path.clone();
        match blocking(move || stamp_of(&probe)).await {
            Ok(stamp) => {
                failures = 0;
                if Some(stamp) != attempted {
                    open(state, path, Reason::Reload);
                }
            }
            Err(error) => {
                failures += 1;
                if failures == 2 {
                    state
                        .note
                        .set(Some(format!("cannot read {}: {error}", path.display())));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use std::time::{Duration, UNIX_EPOCH};
    use xtask_archive::{Archive, Content, Diagram, DiagramKind, Mode};

    fn stamp(seconds: u64, len: u64) -> Stamp {
        Stamp {
            modified: UNIX_EPOCH + Duration::from_secs(seconds),
            len,
        }
    }

    fn module(target: &str) -> DiagramKind {
        DiagramKind::Modules {
            package: "app".into(),
            target: target.into(),
            kind: "lib".into(),
            mode: Mode::Normal,
        }
    }

    /// `kinds` の順に図を持つ文書。内容は試験に要らないので Skipped にする。
    fn loaded(path: &str, kinds: &[DiagramKind]) -> Loaded {
        let diagrams = kinds
            .iter()
            .map(|kind| Diagram {
                kind: kind.clone(),
                title: format!("{kind:?}"),
                content: Content::Skipped("test".into()),
            })
            .collect();
        Loaded {
            path: PathBuf::from(path),
            archive: Archive {
                root_name: "root".into(),
                diagrams,
                notices: Vec::new(),
            },
            pictures: vec![None; kinds.len()],
            kinds: kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| (kind.clone(), index))
                .collect(),
        }
    }

    fn result(path: &str, stamp: Option<Stamp>, loaded: Result<Loaded, String>) -> LoadResult {
        LoadResult {
            path: PathBuf::from(path),
            stamp,
            loaded,
        }
    }

    fn running(id: u64) -> Request {
        Request::Running {
            id,
            path: PathBuf::from("a.xtaskmap"),
            reason: Reason::Open,
        }
    }

    #[test]
    fn open_success_starts_at_crates() {
        let kinds = [DiagramKind::Crates, module("core")];
        let previous = Document::Loaded {
            loaded: Rc::new(loaded("a.xtaskmap", &kinds)),
            attempted: Some(stamp(1, 1)),
        };
        let nav = Nav {
            diagram: module("core"),
            back: vec![DiagramKind::Crates],
            pinned: Some(3),
        };
        let applied = apply(
            &previous,
            &nav,
            Reason::Open,
            result(
                "a.xtaskmap",
                Some(stamp(2, 2)),
                Ok(loaded("a.xtaskmap", &kinds)),
            ),
        );
        assert_eq!(applied.nav, Nav::start());
        assert!(applied.fit);
        assert_eq!(applied.note, None);
    }

    #[test]
    fn reload_keeps_diagram_and_filters_history() {
        let previous = Document::Empty;
        let nav = Nav {
            diagram: module("a"),
            back: vec![DiagramKind::Crates, module("b")],
            pinned: Some(4),
        };
        let kinds = [DiagramKind::Crates, module("a")];
        let applied = apply(
            &previous,
            &nav,
            Reason::Reload,
            result(
                "a.xtaskmap",
                Some(stamp(2, 2)),
                Ok(loaded("a.xtaskmap", &kinds)),
            ),
        );
        assert_eq!(
            applied.nav,
            Nav {
                diagram: module("a"),
                back: vec![DiagramKind::Crates],
                pinned: None,
            }
        );
        assert!(!applied.fit);
        assert_eq!(applied.note, None);
    }

    #[test]
    fn reload_moves_to_crates_when_diagram_disappears() {
        let nav = Nav {
            diagram: module("a"),
            back: Vec::new(),
            pinned: None,
        };
        let kinds = [DiagramKind::Crates];
        let applied = apply(
            &Document::Empty,
            &nav,
            Reason::Reload,
            result(
                "a.xtaskmap",
                Some(stamp(2, 2)),
                Ok(loaded("a.xtaskmap", &kinds)),
            ),
        );
        assert_eq!(applied.nav.diagram, DiagramKind::Crates);
        assert!(applied.fit);
    }

    #[test]
    fn failed_reload_keeps_document_and_records_stamp() {
        let kinds = [DiagramKind::Crates];
        let shared = Rc::new(loaded("a.xtaskmap", &kinds));
        let previous = Document::Loaded {
            loaded: shared.clone(),
            attempted: Some(stamp(1, 1)),
        };
        let applied = apply(
            &previous,
            &Nav::start(),
            Reason::Reload,
            result(
                "a.xtaskmap",
                Some(stamp(2, 2)),
                Err("a.xtaskmap: broken".into()),
            ),
        );
        let expected = Document::Loaded {
            loaded: shared,
            attempted: Some(stamp(2, 2)),
        };
        assert!(applied.document == expected);
        assert!(applied.note.is_some());
        assert!(!applied.fit);
    }

    #[test]
    fn failed_open_over_loaded_keeps_document() {
        let kinds = [DiagramKind::Crates];
        let previous = Document::Loaded {
            loaded: Rc::new(loaded("a.xtaskmap", &kinds)),
            attempted: Some(stamp(1, 1)),
        };
        let applied = apply(
            &previous,
            &Nav::start(),
            Reason::Open,
            result(
                "other.xtaskmap",
                Some(stamp(2, 2)),
                Err("other.xtaskmap: x".into()),
            ),
        );
        assert!(applied.document == previous);
        assert!(applied.note.is_some());
    }

    #[test]
    fn failed_first_open_becomes_failed() {
        let applied = apply(
            &Document::Empty,
            &Nav::start(),
            Reason::Open,
            result(
                "bad.xtaskmap",
                Some(stamp(1, 1)),
                Err("bad.xtaskmap: nope".into()),
            ),
        );
        let Document::Failed {
            path,
            message,
            attempted,
        } = applied.document
        else {
            panic!("a first open that fails is Failed");
        };
        assert_eq!(path, PathBuf::from("bad.xtaskmap"));
        assert_eq!(message, "bad.xtaskmap: nope");
        assert_eq!(attempted, Some(stamp(1, 1)));
        assert_eq!(applied.nav, Nav::start());
        assert!(applied.fit);
    }

    #[test]
    fn stale_completion_is_rejected() {
        assert!(!accepts(&running(2), 1));
        assert!(accepts(&running(2), 2));
        assert!(!accepts(&Request::Idle, 2));
    }

    #[test]
    fn load_rejects_large_files() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("big.xtaskmap");
        let file = std::fs::File::create(&path).expect("a new file");
        file.set_len(xtask_archive::MAX_FILE_LEN + 1)
            .expect("a sparse file");
        let error = load(&path)
            .loaded
            .err()
            .expect("a file over the limit is an error");
        assert!(error.contains("too large"), "{error}");
    }
}
