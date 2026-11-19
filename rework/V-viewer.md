# 付属書 V：viewer（`viewer/`）と CI

設計書 5.5 節の契約を実装する手順。viewer は、このリポジトリの中の**別の Cargo workspace** として置く。Kivact を使うため、Rust のツールチェーンは 1.98.1 にする。

## 0. Kivact の事実（固定する commit で確かめたもの）

- 依存は `kivact = { git = "https://github.com/Kashiwade-music/kivact", rev = "2b8df5aceea40c9ae23950f9d6c942559c89e840" }`。
  - この commit は Kivact の `main`。改修中の branch の API は使わない。
  - 試験では `features = ["test"]` を足す。
- **ソースと文書。** `<KIVACT>`（作業指示書 1.1 節。W0 が固定の commit で clone した directory）のファイルを直接読む。
  - 入門：`docs/for-kivact-user/en/getting-started/`
  - 描画：`docs/for-kivact-user/en/how-to-guide/canvas.md`
  - 配置：`how-to-guide/layout.md`
  - 非同期：`how-to-guide/async-and-tasks.md`
  - ファイルのダイアログ：`how-to-guide/platform-services.md`
  - 試験：`how-to-guide/testing.md`、`dev/tests/kivact-public-api/tests/e2e/parts.rs`
- **よく使う API**（すべて `kivact::prelude::*` か、明記した path にある）：

| 用途 | API |
|---|---|
| 起動 | `App::new(title).window_size(1280, 800).run(|| root())` |
| 状態 | `signal(v) -> Signal<T>`（`get`、`get_untracked`、`with`、`set`、`update`）、`memo(f) -> Value<T>`、`effect(f)`、`var(v) -> Var<T>`（追跡しない値） |
| 非同期 | `spawn(async move { .. })`、`blocking(move || ..).await`、`kivact::task::sleep(Duration).await` |
| 配置 | `row((..))`、`column((..))`、`scroll(child)`、`style().width(280).grow(1).min_width(0).min_height(0).height(pct(100)).gap(8).padding(8)`、`style().position(kivact::style::Position::Absolute).inset(0)` |
| 部品 | `button(label).on_press(..)`、`text_input(label, signal).placeholder(..)`、`select(label, items, selected)`、`text(..)` |
| 木 | `kivact::ui::tree(label, items: Vec<TreeNode<K>>, selected: Signal<Option<K>>)` に `.expanded(signal(vec))`、`.on_activate(|k| ..)` を付ける。node は `kivact::interaction::TreeNode::new(key, parent: Option<K>, label)`。**単クリックは選択だけで、`on_activate` は Enter かダブルクリックで呼ばれる** |
| 描画 | `canvas(move |draw, size| { .. })`（`size` は `kivact::geometry::Size`）。`draw.layer(Layer::new().transform(Transform::new().translate(x, y).scale(s)), |d| ..)`、`draw.fill_path`、`draw.stroke_path(&path, &stroke, paint)`、`draw.text(str, Point, TextFormat)`、`draw.region(RegionId(n), rect)` |
| canvas の大きさ | **canvas は自分の大きさを持たない。** column の中で `.style(style().grow(1).min_height(0))` を付けると残りの高さを得る。`stack((a, b))` は最初の子が大きさを決めるため、canvas を二枚重ねると高さが 0 になる（固定の Kivact で確かめた）。二枚重ねは V2d の手順 1 の形で行う |
| path | `kivact::draw::Path::new().move_to(p).line_to(p).quad_to(c, p).svg_arc_to((r, r), 0.0, false, sweep, p).close()` |
| 線 | `Stroke::new(1.3)`。点線は `Stroke { cap: LineCap::Round, dash: vec![0.0, 3.5], ..Stroke::new(1.3) }` |
| 文字 | `TextFormat { size, family: Some(FONT_FAMILY.into()), align: TextAlign::Center, baseline: TextBaseline::Alphabetic, color, ..TextFormat::default() }`。family は総称の `monospace` 一つ（`,` の列は SVG の出力で一つの名前として扱われる） |
| region | 宣言は `canvas.regions(items, key, make)`。`make: Fn(RegionId, Value<T>) -> Part<S>`。**同じ key の Part は保たれ、item の `Value` だけが新しい値に替わる**（make は key ごとに一回）。Part の名前と handler は、make の時点の値を capture せず、`Value` を読む closure にする。make の Part は `kivact::custom::Part::new(().into_value(), move |host| host.role(Role::Button).name(..).focusable(true).on_key(..))`。描画の中で `draw.region(id, rect)` を呼ぶ。canvas に `.region_events()` を付けると、region の入力が canvas の handler にも届く |
| pointer | `.on_pointer(|e: PointerEvent| ..)`。`e.phase()` は `PointerPhase::{Down(press), Move, Up(press), Cancel, Enter, Leave}`。`press.button` は `MouseButton::{Primary, ..}`、`press.clicks >= 2` がダブルクリック。押すと pointer は canvas に暗黙に capture され、外に出ても Move と Up が届く（Leave は届くが、ボタンを押している間は capture が続く）。`e.pos()` は canvas 座標、`e.window_pos()` は window 座標、`e.region() -> Option<RegionId>` |
| wheel | `.on_wheel(|e: WheelEvent| ..)`。`e.delta().y` が正なら下へ回したとき、`e.pos()` |
| key | `.on_key(|e: KeyEvent| ..)`。`e.key()` は `Key::{Escape, Enter, Space, F(5), Char('0'), Backspace, ..}`。何も focus していないときは root の view に届く |
| ファイル | `kivact::platform::{pick_file, save_file, FileDialog}`（async、`Ok(None)` は取消し）。drag & drop は `.on_file(|e: kivact::view::FileEvent| ..)` で、`e.phase == FilePhase::Drop` のときの `e.files` |
| tooltip | `kivact::view::overlay_at(point_value, Placement::Below, child)` を `show_some` の中で使う |
| SVG | `kivact::draw::Offscreen::new().render_svg(w, h, |draw, size| ..) -> Result<String>`。region は描かない。**Offscreen は `!Send`** なので、`blocking(move || ..)` の closure の中で作る。text は XML の escape がかかる（`->` は `-&gt;`） |
| 試験 | `kivact::test::render(|| view)`、`screen.get_by_role(Role::Button, name).click()`、`.double_click()`、`.rect()`（window 座標の箱）、`screen.press_key(Key::Escape)`、`screen.mouse_move(x, y)`、`mouse_down(MouseButton::Primary)`、`mouse_up(..)`、`screen.advance(Duration)`、`screen.run_until_idle()`、`get_by_text`、`query_by_text` |

- **注意。**
  - canvas の closure の中で signal を書かない（`Var` は書いてよい）。
  - UI の thread で `std::thread::sleep` とファイルの I/O をしない（`blocking` を使う）。
  - 試験は環境変数 `KIVACT_TEST_MIRROR=off` で実行する。複数の試験の process を同時に走らせない。

---

## V1：workspace の骨組み、`xtask-render`、CI

### 所有

- `viewer/Cargo.toml`、`viewer/rust-toolchain.toml`、`viewer/Cargo.lock`
- `viewer/crates/xtask-render/**`
- `viewer/crates/xtask-viewer/Cargo.toml`、`src/lib.rs`、`src/main.rs`、`src/state.rs`、V2 のファイルの骨組み（手順 6）
- `viewer/dev/xtask/policy/workspace.toml`
- `.github/workflows/viewer-windows.yml`

### 手順

1. **`viewer/Cargo.toml`。**

   ```toml
   [workspace]
   members = ["crates/xtask-render", "crates/xtask-viewer"]
   resolver = "3"

   [workspace.package]
   version = "0.1.0"
   edition = "2024"
   license = "MIT OR Apache-2.0"

   [workspace.dependencies]
   kivact = { git = "https://github.com/Kashiwade-music/kivact", rev = "2b8df5aceea40c9ae23950f9d6c942559c89e840" }
   xtask-archive = { path = "../crates/xtask-archive" }
   xtask-render = { path = "crates/xtask-render" }
   ```

   この後に、`<KIVACT>/templates/basic/README.md` にある `[profile.release]` と `[profile.release.package.*]` の block を、そのまま写す。
2. **`viewer/rust-toolchain.toml`。**

   ```toml
   [toolchain]
   channel = "1.98.1"
   profile = "minimal"
   components = ["clippy", "rustfmt"]
   ```

3. **`viewer/crates/xtask-render`。** 依存は `xtask-archive` だけ。Kivact に依存しない純粋な crate にする。図の `Scene` から、描くものの一覧（`Picture`）を作る。入力の `Scene` は `xtask_archive::decode` で検証済み（設計書 5.2 節）であることを前提にしてよい。

   ```rust
   //! Role: 図の Scene から、角の丸め・交差の跳び・矢印・文字の位置を決めた描画の一覧を作る。
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct P { pub x: f64, pub y: f64 }
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct R { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct Rgb(pub u8, pub u8, pub u8);
   #[derive(Clone, Debug, PartialEq)] pub enum Cmd { Move(P), Line(P), Quad { control: P, to: P } }
   #[derive(Clone, Debug, PartialEq)] pub struct Text { pub center_x: f64, pub baseline: f64, pub size: f64, pub text: String }
   #[derive(Clone, Debug, PartialEq)] pub struct NodeShape { pub item: u32, pub rect: R, pub texts: Vec<Text> }
   #[derive(Clone, Debug, PartialEq)] pub struct RegionShape { pub item: u32, pub rect: R, pub fill: Option<Rgb>, pub band: R, pub band_fill: Rgb, pub texts: Vec<Text> }
   #[derive(Clone, Debug, PartialEq)] pub struct EdgeShape { pub from_item: u32, pub to_item: u32, pub dotted: bool, pub path: Vec<Cmd>, pub arrow: [P; 3], pub label: Option<(R, Vec<Text>)> }
   /// 交差の跳び。edge の path の上に、全 edge の後に描く（下の「跳び」）。
   #[derive(Clone, Copy, Debug, PartialEq)] pub struct Bridge { pub edge: u32, pub center: P, pub radius: f64 }
   /// 座標はすべて Picture の座標（Scene の座標 + (16, 42)）。
   #[derive(Clone, Debug, PartialEq)] pub struct Picture { pub width: f64, pub height: f64, pub title: Text, pub regions: Vec<RegionShape>, pub edges: Vec<EdgeShape>, pub bridges: Vec<Bridge>, pub nodes: Vec<NodeShape> }
   pub fn picture(title: &str, scene: &xtask_archive::Scene) -> Picture;
   pub fn neighbors(scene: &xtask_archive::Scene, item: u32) -> Highlight;
   /// item の矩形（Picture の座標）。region があればその region、無ければ node。
   pub fn item_rect(picture: &Picture, item: u32) -> Option<R>;
   #[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct Highlight { pub items: std::collections::BTreeSet<u32>, pub edges: std::collections::BTreeSet<u32> }
   pub mod palette { /* 4 の定数 */ }
   ```

   `picture` の規則は、旧い `crates/xtask-svg/src/render.rs`（`git show e7eb600:crates/xtask-svg/src/render.rs`）を写す。

   - **余白と題。**
     - 全体の大きさは `width = max(scene.width, 題の幅) + 32`、`height = scene.height + 58`。題の幅は `xtask_archive::style::advance` の和。
     - 図の中身はすべて (16, 42) だけずらす。Picture の中の座標は、すべてずらした後の値（Picture の座標）。画面の計算（当たり判定、中央への移動、tooltip、書出し）は Picture の座標だけを使い、Scene の座標を使わない。
     - 題は `center_x = width / 2`、`baseline = 21 + TITLE_FONT_SIZE * 0.35`。
   - **region。** 親から子の順に並べる。
     - 深さは最上位を 0 とする。最上位の `fill` は None、それ以外は `region_background`（深さ d の明るさ `0.8^d`）。
     - `band`（見出しの帯）は `(x + 3, y + 3, width - 6, 行数 * REGION_LINE_HEIGHT + 4)` で、`band_fill` は同じ色。最上位の帯の色は白。
     - 文字は帯の中央にそろえる。
   - **node。** 文字の行は、中央 `(x + w/2, y + h/2)` を基準に旧い `render.rs` の `text` と同じ式で baseline を決める。
   - **edge。**
     - 点列はすでに直交している。各角を半径 `min(4, 前の辺の長さ/2, 次の辺の長さ/2)` の `Quad`（制御点は角）で丸める。path は `Move`、`Line`、`Quad` だけで、跳びは入れない。
     - `dotted` は `EdgeStyle::Dev` のとき true。
     - **矢印。** 最後の点を先端に、最後の線分の向き（必ず右向き）に、長さ `min(8, 最後の線分の長さ)`、半幅 `長さ * 0.4` の三角形を作る。
     - ラベルは、`label.rect` を (−2, −1) 広げた白い矩形と、中央にそろえた文字。
   - **跳び（`bridges`）。** 水平の線分が、別の edge の垂直の線分と内部で交わる点を求める。
     - 探し方：全 edge の垂直の線分を x の昇順に並べ、各水平の線分の x の範囲を二分探索で絞って候補にする（全組を比べない）。
     - 一つの水平の線分の上の交点を x の昇順に並べ、次のものを除く：線分の両端から 8 未満のもの、同じ x のもの（一つにする）、直前に残した交点から `2 * 4 = 8` 未満のもの。
     - 残った交点ごとに `Bridge { edge: 水平の線分の edge, center: 交点, radius: 4.0 }` を作る。並びは edge の index、線分の順、x の昇順。
     - 描き方（V2b の `paint`）は旧い `render.rs` の bridge の描き方を写す：全 edge を描いた後に、各 bridge について、白い halo（`EDGE_WIDTH + 3` の太さで、中心の左右 radius の水平の範囲と半円を覆う）を描き、その上に edge の線の形式（点線を含む）で半円を描く。
   - `neighbors(item)`：
     - items には item 自身を入れる。
     - edges には、端の node の item が item である edge を入れる。
     - items には、それらの edge のもう一方の端の node の item も入れる。
     - 一回の hover の変化ごとに edge を一度たどる（edge は数百本なので索引は作らない）。

4. **palette（`xtask-render` の `pub mod palette`）。**

   | 名前 | 値 |
   |---|---|
   | `STROKE` | `Rgb(0x47, 0x55, 0x69)` |
   | `NODE_STROKE` | `Rgb(0x33, 0x41, 0x55)` |
   | `REGION_STROKE` | `Rgb(0x94, 0xa3, 0xb8)` |
   | `TEXT` | `Rgb(0x0f, 0x17, 0x2a)` |
   | `ACCENT` | `Rgb(0x25, 0x63, 0xeb)` |
   | `BACKGROUND` | `Rgb(0xff, 0xff, 0xff)` |
   | `NODE_FILL` | `Rgb(0xff, 0xff, 0xff)` |

   線の太さは `EDGE_WIDTH = 1.3`、`NODE_STROKE_WIDTH = 1.2`、`REGION_STROKE_WIDTH = 1.0`、`ACCENT_WIDTH = 2.6`。node の角の半径は 2、region の角の半径は 4。

5. **`xtask-render` の試験。**
   - `corner_is_rounded`：L 字の edge の path が `Move, Line, Quad, Line` になる。
   - `crossing_adds_bridge`：十字に交わる 2 本の edge で、`bridges` が 1 個で、その edge が水平の線を持つ edge。
   - `bridge_order_does_not_depend_on_edge_order`：上の 2 本の edge の順を入れ替えても、`bridges` の center が同じ。
   - `close_crossings_are_merged`：水平の線分 (0, 50)→(60, 50) を、x = 20、22、40 の垂直の線分が横切るとき、bridge の center の x が `[20, 40]`（22 は 20 から 8 未満なので除く）。
   - `arrow_points_right`：矢印の先端が最後の点で、残りの二点の x が先端より小さい。
   - `neighbors_include_both_ends`：a→b の edge で、`neighbors(a)` の items が {a, b}、edges が {0} になる。
   - `title_widens_picture`：幅の小さい scene と長い題で、`width` が題の幅 + 32 以上になる。
   - `item_rect_uses_picture_coordinates`：Scene の node (0, 0, 10, 10) の `item_rect` が (16, 42, 10, 10)。

6. **`xtask-viewer` の骨組み。**
   - `Cargo.toml`：
     - 依存は `kivact.workspace = true`、`xtask-archive.workspace = true`、`xtask-render.workspace = true`。
     - `[dev-dependencies]` に `kivact = { workspace = true, features = ["test"] }` と `tempfile = "3"` を足す。
     - binary の名前は `xtask-viewer`。
   - crate は library と binary の両方を持つ。試験が library の module を呼ぶため。
     - `src/lib.rs` には `//! Role:` の行と、すべての module の `pub mod` を書く（`app`、`camera`、`canvas`、`document`、`export`、`navigate`、`paint`、`sidebar`、`state`）。
     - `src/main.rs` には、`//! Role:` の行と次の一つだけを書く。

       ```rust
       fn main() -> kivact::Result<()> {
           kivact::prelude::App::new("xtask viewer")
               .window_size(1280, 800)
               .run(|| xtask_viewer::app::root(std::env::args().nth(1).map(std::path::PathBuf::from)))
       }
       ```

   - V2 の表の関数は、すべて `pub fn` にする。
   - V2 の lane が `lib.rs` を触らないよう、V1 は V2 の表のすべてのファイル（`app.rs`、`document.rs`、`export.rs`、`canvas.rs`、`camera.rs`、`paint.rs`、`navigate.rs`、`sidebar.rs`）を作る。
     - 中身は、`//! Role:` の行、表の公開の関数・型の署名と `todo!("<lane>")` の本体、`paint::Mode` の enum だけにする。表に「このとおり」とある型は、V1 が書く。
     - `todo!()` の骨組みは `dead_code`・`unused_variables` の警告を出す。V2 の lane が本体を書くまで、この警告は残してよい（作業指示書 0.6 節）。
7. **`src/state.rs`（このとおりに書く。V2 の lane が共有する）。**

   ```rust
   //! Role: viewer の画面の状態（文書・読込み・表示中の図・視点・注目）を明示の型で定義する。
   use std::collections::BTreeMap;
   use std::path::PathBuf;
   use std::rc::Rc;
   use std::time::SystemTime;
   use kivact::prelude::*;
   use xtask_archive::DiagramKind;

   /// 開いているファイルの状態。`attempted` は、最後に読もうとした時のファイルの印（読めなかった時を含む）。
   #[derive(Clone)]
   pub enum Document {
       Empty,
       Loaded { loaded: Rc<Loaded>, attempted: Option<Stamp> },
       Failed { path: PathBuf, message: String, attempted: Option<Stamp> },
   }

   impl PartialEq for Document {
       fn eq(&self, other: &Self) -> bool {
           match (self, other) {
               (Self::Empty, Self::Empty) => true,
               (Self::Loaded { loaded: a, attempted: x }, Self::Loaded { loaded: b, attempted: y }) => Rc::ptr_eq(a, b) && x == y,
               (Self::Failed { path: a, message: m, attempted: x }, Self::Failed { path: b, message: n, attempted: y }) => a == b && m == n && x == y,
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
   pub struct Stamp { pub modified: SystemTime, pub len: u64 }

   /// 読込みの理由。
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum Reason { Open, Reload }

   /// 走っている読込みの要求。表示（Document）とは別の事実。新しい要求は古い要求の結果を捨てさせる。
   #[derive(Clone, Debug, PartialEq, Eq)]
   pub enum Request { Idle, Running { id: u64, path: PathBuf, reason: Reason } }

   /// 表示中の図・履歴・固定の強調。`navigate` と `document::apply` だけが作る。
   #[derive(Clone, Debug, PartialEq, Eq)]
   pub struct Nav { pub diagram: DiagramKind, pub back: Vec<DiagramKind>, pub pinned: Option<u32> }

   impl Nav {
       /// 開いた直後：crate 図、空の履歴、固定なし。
       pub fn start() -> Self { Self { diagram: DiagramKind::Crates, back: Vec::new(), pinned: None } }
   }

   /// 視点。Fit は canvas の大きさに合わせて描画のたびに求める。
   /// Manual では、画面の点 = Picture の点 * scale + offset。
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub enum Camera {
       Fit,
       Manual { offset_x: f64, offset_y: f64, scale: f64 },
   }

   /// 実際の視点（Fit を解いた値）。
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub struct View { pub offset_x: f64, pub offset_y: f64, pub scale: f64 }

   /// 押した後の pointer の扱い。canvas の handler だけが読み書きする（追跡しない Var）。
   #[derive(Clone, Copy, Debug, PartialEq)]
   pub enum Gesture {
       Idle,
       /// 押したが、まだ 3 px 動いていない。`target` は押した region の item。
       Pressed { start: (f64, f64), view: View, target: Option<u32> },
       /// 3 px 以上動いた。視点を動かしている。
       Dragging { start: (f64, f64), view: View },
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
           let Document::Loaded { loaded, .. } = self.document.get() else { return None };
           let index = self.nav.with(|nav| loaded.kinds.get(&nav.diagram).copied()).unwrap_or(0);
           Some((loaded, index))
       }
   }
   ```

8. **`viewer/dev/xtask/policy/workspace.toml`。**

   ```toml
   [limits]
   max_code_lines = 1000

   [analysis]
   # kivact の prelude を未知の提供元とみなさない。prelude は attribute の component も与えるが、
   # viewer は使わない。使うときは trusted_attributes にも書く（設計書 DEC-9）。
   trusted_globs = ["kivact::prelude"]

   [visible_to]
   xtask-render = ["xtask-viewer"]
   ```

9. **CI（`.github/workflows/viewer-windows.yml`）。** main に push があったときだけ build する。

   ```yaml
   name: viewer-windows
   on:
     push:
       branches: [main]
   permissions:
     contents: read
   jobs:
     build:
       runs-on: windows-latest
       steps:
         - uses: actions/checkout@v4
         - name: Install the viewer toolchain
           working-directory: viewer
           run: rustup toolchain install
         - uses: Swatinem/rust-cache@v2
           with:
             workspaces: viewer
         - name: Build
           working-directory: viewer
           run: cargo build --release --locked -p xtask-viewer
         - uses: actions/upload-artifact@v4
           with:
             name: xtask-viewer-windows
             path: viewer/target/release/xtask-viewer.exe
   ```

10. **lock。** `(cd viewer && cargo generate-lockfile)` を実行し、`viewer/Cargo.lock` を commit する。初回は network が要る。network が無ければ STOP にする。

### 確認

```bash
(cd <ROOT>/viewer && cargo check -p xtask-render --all-targets && cargo test -p xtask-render && cargo check -p xtask-viewer)
```

最後の check は、骨組みの `todo!()` の本体で通ればよい。

---

## V2：viewer の画面（V2a、V2b → V2d、V2c）

lane は別々のファイルを所有する。共有するのは V1 の `state.rs` と、下の関数の署名だけ。署名はこのとおりに書く（V1 が骨組みに置く）。他の lane の関数は `todo!()` のまま呼んでよい（実行はしない）。

| unit | 所有するファイル | 公開する関数・型 |
|---|---|---|
| V2a | `src/app.rs`、`src/document.rs`、`src/export.rs` | `app::root(initial: Option<PathBuf>) -> impl IntoView`、`app::view(state: AppState, initial: Option<PathBuf>) -> impl IntoView`、`document::load(path: &Path) -> LoadResult`、`document::apply(previous: &Document, nav: &Nav, reason: Reason, result: LoadResult) -> Applied`、`document::accepts(request: &Request, id: u64) -> bool`、`document::open(state: AppState, path: PathBuf, reason: Reason)`、`document::watch(state: AppState)`、`export::export_svg(state: AppState)`、型 `LoadResult`、`Applied`（下のとおり） |
| V2b | `src/paint.rs`、`src/camera.rs`、`src/navigate.rs` | `paint::picture(draw: &mut Draw, picture: &Picture, mode: Mode)`、`paint::highlight(draw: &mut Draw, picture: &Picture, highlight: &Highlight)`、`camera::view(camera: Camera, picture: &Picture, size: (f64, f64)) -> View`、`camera::zoom(view: View, anchor: (f64, f64), delta_y: f64) -> Camera`、`camera::pan(view: View, start: (f64, f64), pos: (f64, f64)) -> Camera`、`camera::center(rect: R, scale: f64, size: (f64, f64)) -> Camera`、`navigate::open(nav: &Nav, loaded: &Loaded, diagram: usize) -> Nav`、`navigate::back(nav: &Nav) -> Nav`、`navigate::toggle_pin(nav: &Nav, item: u32) -> Nav`、`navigate::unpin(nav: &Nav) -> Nav`、`navigate::go(state: AppState, diagram: usize)`、`navigate::go_back(state: AppState)`、`navigate::fit(state: AppState)`、`navigate::reveal(state: AppState, diagram: usize, item: u32)` |
| V2d | `src/canvas.rs` | `canvas::diagram_view(state: AppState) -> impl IntoView` |
| V2c | `src/sidebar.rs` | `sidebar::sidebar(state: AppState) -> impl IntoView`、`sidebar::rows(loaded: &Loaded, search: &str) -> Vec<Row>`、`sidebar::action(loaded: &Loaded, key: TreeKey) -> Option<Action>`、型 `TreeKey`、`Row`、`Action`（下のとおり） |

```rust
// document.rs（V1 がこのとおりに書く）
pub struct LoadResult { pub path: PathBuf, pub stamp: Option<Stamp>, pub loaded: Result<Loaded, String> }
pub struct Applied { pub document: Document, pub nav: Nav, pub fit: bool, pub note: Option<String> }

// sidebar.rs（V1 がこのとおりに書く）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TreeKey { Package(u32), Diagram(u32), Item { diagram: u32, item: u32 } }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row { pub key: TreeKey, pub parent: Option<TreeKey>, pub label: String }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Open(usize), Reveal(usize, u32) }
```

純粋な関数（`document::apply`、`document::accepts`、`camera::*`、`navigate` の `Nav` を返す関数、`sidebar::rows`、`sidebar::action`）は、各 lane が自分のファイルの `#[cfg(test)] mod tests` で試験する（GUI を起動しない）。

### V2a：枠・文書・書出し

1. **`app::root` と `app::view`。**
   - `root(initial)` は `view(AppState::new(), initial)` を返すだけ。試験は `view` に自分の `AppState` を渡す。
   - `view` は、`initial` が Some なら `document::open(state, path, Reason::Open)` を呼ぶ。`document::watch(state)` は、`initial` の有無にかかわらず一度だけ呼ぶ。
   - toolbar の先頭に、今の図の題を `text(move || ..)` で出す。文書が Loaded でなければ、`No file open` か失敗の文を出す（V3 の試験が文で探す）。
   - 画面は `row((sidebar::sidebar(state), column((toolbar, canvas::diagram_view(state), notices))))` にする。
     - sidebar の幅は 300 にする。右の column は `grow(1).min_width(0)` にする。全体は `height(pct(100)).width(pct(100))` にする。
     - toolbar は、`Open…`（`pick_file` の後に `document::open(.., Reason::Open)`）、`Reload`（今の path で `Reason::Reload`）、`Back`、`Fit`、`Export SVG` の button と、同じ package の図を切り替える `select` を並べる。
       - select の項目は、`archive.diagrams` のうち今の図と同じ package の Modules の図。label は `"{target} ({kind}, {normal|test})"`。今の図が Crates なら項目は無い。
       - 選ぶと `navigate::go` を呼ぶ。
     - notices は、`archive.notices`、今の図が Failed・Skipped ならその文、`note` の文を、`column` に `text` で並べる。
   - root の `.on_file` は、Drop のとき最初のファイルで `document::open(.., Reason::Open)` を呼ぶ。
   - root の `.on_key` は次のとおり。どれも `Handled::Yes` を返し、それ以外は `Handled::No` を返す。

     | key | 動作 |
     |---|---|
     | `Key::Escape` | `nav = navigate::unpin(&nav)` |
     | `Key::F(5)` | 今の path（Loaded の `loaded.path` か Failed の `path`）で `document::open(.., Reason::Reload)` |
     | `Key::Char('0')` | `navigate::fit` |
     | `Key::Backspace` | `navigate::go_back` |

2. **`document::load(path) -> LoadResult`。** UI の thread の外（`blocking`）で呼ぶ前提の、純粋な関数にする。
   - `std::fs::metadata` で Stamp を作る（失敗なら `stamp: None` で、`loaded` は `Err`）。
   - 長さが `xtask_archive::MAX_FILE_LEN` を超えれば `Err("{path}: file is too large")`。
   - `std::fs::read` で読み、`xtask_archive::decode` で復号する。
   - Drawn の図ごとに `xtask_render::picture(&diagram.title, scene)` を作る。`kinds` の索引を作る。
   - 誤りは `"{path}: {error}"` の文にする。
3. **`document::apply`（純粋な関数。読込みの結果を状態に写す規則の唯一の場所）。**

   | 結果 | previous | reason | document | nav | fit | note |
   |---|---|---|---|---|---|---|
   | Ok | 何でも | Open | `Loaded { loaded, attempted: stamp }` | `Nav::start()` | true | None |
   | Ok | 何でも | Reload | `Loaded { loaded, attempted: stamp }` | 下の「読直しの nav」 | 図が替わったときだけ true | None |
   | Err | `Loaded { loaded, .. }` | Open | previous のまま | 今の nav | false | `Some(文)` |
   | Err | `Loaded { loaded, attempted }` | Reload | `Loaded { loaded, attempted: stamp.or(attempted) }` | 今の nav | false | `Some(文)` |
   | Err | `Empty` か `Failed` | 何でも | `Failed { path, message: 文, attempted: stamp }` | `Nav::start()` | true | None |

   **読直しの nav**：`diagram` は新しい `kinds` にあれば保ち、無ければ `Crates`。`back` は新しい `kinds` にある kind だけを残す（順は保つ）。`pinned` は None（item の index が変わりうるため）。
4. **`document::accepts(request, id)`**：`request` が `Running { id: 同じ id, .. }` なら true。
5. **`document::open(state, path, reason)`。**
   - `id = state.last_request.get() + 1` を `last_request` に書き、`state.request.set(Request::Running { id, path, reason })`。
   - `spawn` の中で `let result = blocking(move || load(&path)).await;` を待つ。
   - 戻ったら、`accepts(&state.request.get_untracked(), id)` が false なら何もしない（新しい要求に追い越された）。true なら `request = Idle` にし、`apply` の結果を `document`、`nav` に書き、`fit` なら `camera = Fit`、`note` があれば書き、`hover = None` にする。
   - `blocking` の結果（`LoadResult`）は `Send`。`Loaded` は `Rc` を持たない。
6. **`document::watch(state)`。** `spawn` の中で次を繰り返す。失敗の回数は、この loop の局所の変数 `failures: u32` で数える。
   - `kivact::task::sleep(Duration::from_millis(500)).await` で待つ。
   - `request` が `Running` なら、この回は何もしない（同じ変更を重ねて読まない）。
   - `document` が `Loaded` か `Failed` なら、その path と `attempted` を取る。`Empty` なら何もしない。
   - Stamp を `blocking` で取る。
     - 取れたら `failures = 0`。`Some(stamp) != attempted` なら `document::open(state, path, Reason::Reload)`。
     - 取れなければ `failures += 1`。`failures == 2` のときだけ `note = Some("cannot read {path}: {error}")`（書換えの途中の一瞬の失敗は通知しない。続く失敗は一度だけ通知する。戻ったら印が変わるので読み直し、成功で note が消える）。
7. **`export::export_svg(state)`。** 今の図の Picture が無ければ何もしない。
   - その時点の Picture を clone する（書出しの対象はクリックした時の図。後で文書が替わっても変わらない）。
   - `save_file(FileDialog { title: "Export SVG".into(), file_name: Some(format!("{}.svg", 題を英数字と '-' 以外を '_' にしたもの)), filters: vec![("SVG".into(), vec!["svg".into()])], ..Default::default() })` を呼ぶ。
   - path が得られたら、`blocking(move || { let svg = Offscreen::new().render_svg(picture.width, picture.height, |draw, _| paint::picture(draw, &picture, paint::Mode::Export))?; std::fs::write(&path, svg) })` で、SVG の生成と書込みを両方とも UI の外で行う（Offscreen は `!Send` なので closure の中で作る）。
   - 失敗は `note` に入れる。

#### 試験（`document.rs` と `app.rs` の単体試験）

- `open_success_starts_at_crates`：previous が Loaded（module 図を表示、back が 1 個）で、Open の Ok の結果を `apply` すると、nav が `Nav::start()`、fit が true。
- `reload_keeps_diagram_and_filters_history`：今の図が A、back が `[Crates, B]`。読み直した archive に A と Crates はあるが B が無い。結果の nav は diagram が A、back が `[Crates]`、pinned が None、fit が false。
- `reload_moves_to_crates_when_diagram_disappears`：今の図が消えた読直しで、diagram が Crates、fit が true。
- `failed_reload_keeps_document_and_records_stamp`：Loaded（attempted が s1）に Reload の Err（stamp が s2）を `apply` すると、同じ `Rc` の Loaded で attempted が s2、note が Some。
- `failed_open_over_loaded_keeps_document`：Loaded に別の path の Open の Err を `apply` すると、document が previous と等しく、note が Some。
- `failed_first_open_becomes_failed`：Empty に Err を `apply` すると `Failed`。
- `stale_completion_is_rejected`：`accepts(&Running { id: 2, .. }, 1)` が false、`accepts(&Running { id: 2, .. }, 2)` が true、`accepts(&Idle, 2)` が false。
- `load_rejects_large_files`：`MAX_FILE_LEN + 1` バイトのファイル（sparse でよい。`File::set_len`）の `load` が Err で、文に `too large` を含む。

#### 確認

```bash
(cd <ROOT>/viewer && cargo check -p xtask-viewer --all-targets && cargo test -p xtask-viewer --lib document:: && cargo test -p xtask-viewer --lib app::)
```

（`app::` に試験が無ければ、2 つ目の test は省く。0 件の filter は失敗とみなすため。）

### V2b：描画・視点・図の移動（純粋な部分）

1. **`paint::Mode`。** `pub enum Mode { Screen, Export }`。Export のときは `draw.region` を呼ばない。
2. **`paint::picture`。** 描く順は次のとおり。
   1. 背景（白、全体）。
   2. 題。
   3. region の矩形（`REGION_STROKE` の 1.0 の線、角の半径 4、fill）。
   4. edge の path（`STROKE`、`EDGE_WIDTH`、dotted は点線）と矢印（`STROKE` の fill）。
   5. 跳び（`bridges`。V1 の手順 3 の描き方）。
   6. node の矩形（白の fill と `NODE_STROKE` の 1.2 の線、角の半径 2）と文字。
   7. region の帯（band の fill）と見出しの文字。
   8. edge のラベル（白い矩形と文字）。

   文字は `TextFormat { size, family: Some(xtask_archive::style::FONT_FAMILY.into()), align: TextAlign::Center, baseline: TextBaseline::Alphabetic, color: palette::TEXT, ..Default::default() }` で、行ごとに `draw.text(行, Point::new(center_x, baseline), format)` を呼ぶ。

   Mode が Screen のときは、hit の範囲を次のとおり描く。
   - Picture に region が一つでもあれば、region ごとに `draw.region(RegionId(index as u64), rect)` を親から子の順に描く。
   - region が無ければ、node ごとに描く。
   - RegionId の値は、region または node の index。
3. **`paint::highlight`。** `Highlight` の edge の path を `ACCENT`・`ACCENT_WIDTH` で描き直し、その edge の bridge も `ACCENT` で描き直す。node と region の矩形の輪郭を `ACCENT` の 2.0 の線で描く。
4. **`camera`（純粋な関数。座標はすべて Picture の座標と canvas の座標）。**
   - `view(camera, picture, size)`：Fit なら `scale = min(1.0, min(w / pw, h / ph) * 0.95)` とし、Picture が canvas の中央に来る offset にする。Manual ならその値。
   - `zoom(view, anchor, delta_y)`：`f = (-delta_y * 0.0015).exp()`、`s' = clamp(view.scale * f, 0.05, 8.0)`、`offset' = anchor - (anchor - offset) * (s' / view.scale)`（x と y のそれぞれ）。`Camera::Manual` を返す。
   - `pan(view, start, pos)`：`offset = view.offset + (pos - start)`、scale は `view.scale`。
   - `center(rect, scale, size)`：rect の中心が `size` の中央に来る offset と、その scale の `Camera::Manual`。
5. **`navigate`（`Nav` を返す純粋な関数と、状態に書く関数）。**
   - `open(nav, loaded, d)`：`d` の kind が今の図と同じなら pinned だけ None にして返す。違えば、今の kind を `back` に push し、`diagram = kind(d)`、`pinned = None`。
   - `back(nav)`：`back` が空ならそのまま。そうでなければ pop して `diagram` にし、`pinned = None`。
   - `toggle_pin(nav, item)`：`pinned == Some(item)` なら None、それ以外は `Some(item)`。`unpin(nav)`：`pinned = None`。
   - `go(state, d)`：`nav = open(..)`、`camera = Fit`、`hover = None`。
   - `go_back(state)`：`nav = back(..)`、`camera = Fit`、`hover = None`。
   - `fit(state)`：`camera = Camera::Fit`。
   - `reveal(state, d, item)`：`nav = open(&nav, loaded, d)` の後に `pinned = Some(item)` にする（toggle ではない）。scale は今の `camera::view` の値を保ち、`xtask_render::item_rect(picture, item)` の矩形で `camera = center(rect, scale, viewport)`。`hover = None`。

#### 試験（`camera.rs` と `navigate.rs` の単体試験）

- `fit_centers_picture`：Picture 200×100、canvas 400×400 で、scale が 1.0、offset が (100, 150)。
- `zoom_keeps_anchor_fixed`：view (offset 10, 20、scale 1)、anchor (110, 120)、delta_y −100 の後、anchor の下の Picture の点（(100, 100)）が、新しい view でも anchor に写る（誤差 1e-9）。
- `pan_moves_by_pointer_delta`：start (0, 0)、pos (30, −10) で、offset が 30、−10 だけ動き、scale は同じ。
- `center_puts_rect_in_middle`：rect (16, 42, 10, 10)、scale 2、size (400, 300) で、rect の中心 (21, 47) が (200, 150) に写る。
- `open_pushes_history_and_clears_pin`、`back_pops_and_clears_pin`、`back_on_empty_history_is_unchanged`、`toggle_pin_twice_unpins`。

#### 確認

```bash
(cd <ROOT>/viewer && cargo check -p xtask-viewer --all-targets && cargo test -p xtask-viewer --lib camera:: && cargo test -p xtask-viewer --lib navigate::)
```

### V2d：図の canvas（V2b の後、同じ lane）

1. **重ね方。** 二つの canvas（`base` と `overlay`）を、次の形で重ねる（固定の Kivact で、両方が残りの大きさを得ることを確かめた形。`stack` は使わない）。

   ```rust
   column((
       base.style(style().grow(1).min_height(0)),
       overlay
           .style(style().position(kivact::style::Position::Absolute).inset(0))
           .hit_testable(false),
   ))
   .style(style().grow(1).min_height(0).position(kivact::style::Position::Relative))
   ```

   `base` には `.name("Diagram")` を付ける（試験が `Role::Canvas` と名前で探す）。
2. **base。**
   - 読むのは `document`、`nav` の `diagram`（`memo(move || state.nav.with(|n| n.diagram.clone()))` を通し、固定の変化で描き直さない）、`camera` だけ。
   - 描画の closure の最初で `state.viewport.set((size.width, size.height))` を呼ぶ（`Var` は追跡しない値なので、描画の中で書いてよい）。Kivact がこれを誤りとして報告したら STOP する。
   - `camera::view(camera, picture, size)` で実際の視点を求める。
   - `Layer::new().transform(Transform::new().translate(view.offset_x, view.offset_y).scale(view.scale))` の中で、`paint::picture(draw, picture, Mode::Screen)` を呼ぶ。
3. **overlay。** 読むのは `document`、`nav`、`camera`、`hover` だけ。強調する item は `nav.pinned`、無ければ `hover`。どちらかがあるときだけ、同じ transform の中で `paint::highlight(draw, picture, &xtask_render::neighbors(scene, item))` を呼ぶ。
4. **region の宣言。** base に `.regions(entries, key, make)` で行う。
   - entries は `memo` で作る `Vec<RegionEntry>`（`#[derive(Clone, PartialEq)] struct RegionEntry { id: u64, item: u32, name: String, open: Option<u32> }`）。今の図の region（無ければ node）ごとに一つ。key は `RegionId(entry.id)`。
   - make は `move |_, entry: Value<RegionEntry>|` で、Part の名前と handler は **`entry` を読む closure** にする（`.name(move || entry.with(|e| e.name.clone()))`、handler の中でも `entry.with(..)` で item と open を読む）。make の引数の値を変数に取り出して capture しない（同じ index の region が別の図や読直しで別の item になっても、名前と handler が新しい値に従うため）。
   - Part は `role(Role::Button).focusable(true).selected(move || state.nav.with(|n| n.pinned) == Some(item))`（item は `entry` から読む）に、次の `on_key` を付ける。
     - `Key::Enter`：item の `open` が Some(d) なら `navigate::go(state, d)`。
     - `Key::Space`：`nav = navigate::toggle_pin(&nav, item)`。
     - どちらも `Handled::Yes`、他は `Handled::No`。
   - base に `.region_events()` を付ける。
5. **pointer。** base の `.on_pointer`。gesture は `state.gesture`（`Var<Gesture>`）。押した item は `e.region()` の RegionId から entries で引く。

   | phase | 条件 | 動作 |
   |---|---|---|
   | `Down(p)` | `p.button` が Primary でない | 何もしない |
   | `Down(p)` | `p.clicks >= 2` で item に当たり、その `open` が Some(d) | `navigate::go(state, d)`。gesture は Idle |
   | `Down(p)` | それ以外 | gesture を `Pressed { start: pos, view: 今の view, target: 当たった item }` |
   | `Move` | `Pressed` で、start からの距離が 3 以上 | gesture を `Dragging { start, view }` にし、`camera = camera::pan(view, start, pos)` |
   | `Move` | `Dragging` | `camera = camera::pan(view, start, pos)` |
   | `Move` | `Idle` | `hover` を当たった item（無ければ None）にし、`pointer` を `e.window_pos()` にする |
   | `Up(p)` | `Pressed { target: Some(i) }` | `nav = toggle_pin(&nav, i)`。gesture は Idle |
   | `Up(p)` | `Pressed { target: None }` | `nav = unpin(&nav)`。gesture は Idle |
   | `Up(p)` | `Dragging` | gesture は Idle（視点はそのまま） |
   | `Leave` | — | `hover = None`（gesture は変えない。押している間は capture が続く） |
   | `Cancel` | — | gesture は Idle、`hover = None` |

6. **wheel。** gesture が Idle でなければ何もせず `Handled::Yes`。Idle なら、今の view で `camera = camera::zoom(view, e.pos(), e.delta().y)` にし、`Handled::Yes`。
7. **tooltip。** `hover` が Some(item) のとき（固定の有無にかかわらず）、`pointer` の位置に `overlay_at(.., Placement::Below, text(..))` を出す。文は item の name、role があれば改行して role。

#### 確認

```bash
(cd <ROOT>/viewer && cargo check -p xtask-viewer --all-targets)
```

### V2c：sidebar

1. **`rows(loaded, search)`（純粋な関数）。**
   - **package の一覧**：crate 図が Drawn ならその item の名前と、すべての Modules の図の `package` の和を、名前の昇順に重複なく並べる（crate 図が Failed でも、module 図の package から木を作る）。`Package(i)` の `i` はこの一覧の index。label は package 名。
   - その下に、その package の Modules の図ごとに `Diagram(d)`（`d` は図の index）。label は `"{target} ({kind}, {normal|test})"`。Failed・Skipped の図も出す（label の後ろに ` — failed` か ` — skipped`）。
   - Drawn の Modules の図では、その下に item ごとに `Item { diagram: d, item }`。parent は、その item の region の親の region の item。最上位は `Diagram(d)`。label は item の name。
   - **絞込み**：search を小文字にした文字列が空でなければ、label の小文字が search を含む row と、その祖先だけを残す。
2. **`action(loaded, key)`（純粋な関数）。**
   - `Package(i)`：crate 図が Drawn で、その package の item の `open` が Some(d) なら `Open(d)`。そうでなければ、その package の最初の Modules の図（archive の順）を `Open`。どちらも無ければ None。
   - `Diagram(d)`：`Open(d)`。
   - `Item { diagram, item }`：`Reveal(diagram, item)`。
3. **画面。** `column((text_input("Search", state.search).placeholder("Filter crates and modules"), scroll(tree).style(style().grow(1).min_height(0))))` にする。
   - tree の items は `rows` を `memo`（`document` と `search` に依存）で `TreeNode::new(row.key, row.parent, row.label)` に写したもの。
   - 絞込み中は、残した row をすべて展開する（`expanded` に入れる）。
   - `on_activate(|key| ..)`（Enter かダブルクリック）：`action` が `Open(d)` なら `navigate::go(state, d)`、`Reveal(d, item)` なら `navigate::reveal(state, d, item)`。単クリックは選択だけ（設計書 DEC-21）。

#### 試験（`sidebar.rs` の単体試験）

試験の Loaded は、`xtask_archive` の型で手で作った Archive から作る（`pictures` と `kinds` も手で作る helper を、この試験の中に書く）。

- `rows_follow_packages_diagrams_and_modules`：crate 図に item a・b、a の lib の Normal の module 図（module `crate`、`crate::x`）がある Archive で、row の key の列が `[Package(0), Diagram(1), Item{1,0}, Item{1,1}, Package(1)]`、`Item{1,1}` の parent が `Item{1,0}`。
- `failed_crate_diagram_still_lists_packages`：crate 図が Failed のとき、Modules の図の package（a）の row がある。
- `search_keeps_matches_and_ancestors`：search `x` で、`Item{1,1}` と、その祖先（`Item{1,0}`、`Diagram(1)`、`Package(0)`）だけが残り、`Package(1)`（b）が消える。
- `actions_open_and_reveal`：`action(Package(0))` が `Open(1)`、`action(Item{1,1})` が `Reveal(1, 1)`。

#### 確認

```bash
(cd <ROOT>/viewer && cargo check -p xtask-viewer --all-targets && cargo test -p xtask-viewer --lib sidebar::)
```

---

## V3：viewer の GUI 試験（merge の後、1 lane）

### 所有

`viewer/crates/xtask-viewer/tests/viewer.rs`

### 手順

試験用の Archive を `xtask_archive` の型で手で作る。
- crate 図：item `a`（open = 1）、`b`。node 2 個、edge a→b。
- module 図（index 1）：module `crate`、`crate::x`（region 2 個、node 2 個）。edge `crate::x`→`crate`。

これを `tempfile` の一時 directory に `encode` して書き、`kivact::test::render(move || app::view(state, Some(path)))` で開く。`state` は試験の中で `AppState::new()` を render の closure の中で作り、外へは `Rc<Cell<Option<AppState>>>` で渡す（`AppState` は Copy）。

GUI の試験は結線と、実際の大きさ・gesture が成り立つことだけを確かめる。規則の細部は V2 の単体試験が確かめている。

- **`opens_file_from_argument`**：`run_until_idle` の後、題 `Workspace crate dependencies (consumer -> provider)` が見つかる。
- **`canvas_has_positive_size`**：`get_by_role(Role::Canvas, "Diagram").rect()` の幅と高さが 100 より大きい。
- **`click_pins_and_escape_clears`**：`get_by_role(Role::Button, "a").click()` の後、その locator の `is_selected()` が true になる。`press_key(Key::Escape)` の後は false になる。
- **`double_click_opens_module_diagram_and_renames_regions`**：`get_by_role(Role::Button, "a").double_click()` の後、`crate::x` の button が見つかり、`b` の button が無い（同じ index の region の名前が新しい図に従う）。
- **`drag_on_region_pans`**：`a` の button の `rect()` の中心で `mouse_move`、`mouse_down(Primary)`、40 px 右へ `mouse_move`、`mouse_up` の後、`state.camera` が `Manual` で、`a` が固定されていない（ドラッグはクリックにならない）。
- **`search_filters_tree`**：search に `x` を入れると、tree に `crate::x` が残り、`b` が消える。
- **`reload_after_change`**：ファイルを、題を変えた Archive で書き直す。`advance(Duration::from_millis(600))` と `run_until_idle()` の後、新しい題が見える。
- **`export_svg_matches_screen_paint`**：`Offscreen::new().render_svg(..)` に `paint::picture(.., Mode::Export)` を渡した文字列が、`<svg`、XML の escape をかけた題 `Workspace crate dependencies (consumer -&gt; provider)`、`font-family="monospace"` を含む。

### 確認

```bash
(cd <ROOT>/viewer && KIVACT_TEST_MIRROR=off cargo test -p xtask-viewer --test viewer)
(cd <ROOT>/viewer && cargo clippy --workspace --all-targets -- -D warnings)
```

GUI の試験は、すべて書き終えてから 1 回だけ実行する。失敗したら原因を直し、もう 1 回だけ実行してよい（作業指示書 4 章）。clippy の指摘が V2 の lane のファイルにあれば、作業指示書 0.4 節の「STOP にせず直すもの」の範囲で直してよい（W4 では V2 の lane は終わっているので、所有の衝突は無い）。
