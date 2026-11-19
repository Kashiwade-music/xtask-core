# xtask-core 改修 作業指示書

版 1.1（2026-11-19）。設計の正本は [REWORK_DESIGN.md](REWORK_DESIGN.md)（以下「設計書」）、unit の手順は付属書 `rework/*.md`（以下「付属書」）。本書は**進め方だけ**を書く。設計の判断は設計書と付属書が持つ。

目標：設計書 1.2 節の AC-1〜AC-7 をすべて満たし、設計書 2 章の KPI に届く。

---

## 0. 読み方と規則

### 0.1 誰が何をするか

| 役割 | 担当 | すること |
|---|---|---|
| Orchestrator | 本書を受け取った AI（親） | wave を順に進める。lane ごとに worktree を作り、unit ごとに subagent を起動する。結果を merge し、gate を実行し、記録を commit して push する。subagent の `STOP:` に 0.5 節の表で答える。**設計の判断をしない** |
| Implementer | subagent（**同時に最大 6 体**） | 割り当てられた unit を、指定の worktree の中で、付属書の手順どおりに実装する。確認のコマンドを実行し、結果を報告する |

subagent の起動：

| 実行環境 | 起動の方法 | モデル |
|---|---|---|
| Claude Code | Agent tool を `subagent_type: "general-purpose"`、`model: "haiku"`、`effort: "high"` で呼ぶ | Haiku 5.5、effort high |
| その他 | その環境の subagent の起動の道具に、1.2 節の雛形をそのまま渡す | ユーザーの指定 |

- 同時に動かすのは最大 6 体（lane ごとに 1 体）。subagent に subagent を作らせない。
- 一つの lane に unit が複数あるとき（4 章の表で `→` でつないだもの）は、前の unit が終わってから、同じ worktree で次の unit の subagent を起動する。
- 同じ wave の lane は、互いに独立している（所有するファイルが重ならない）。全 lane を同時に起動してよい。
  - 唯一の例外は W1 の root の `Cargo.toml` の `[workspace.dependencies]`。A1 と M1 がそれぞれ別の行を足し、1.3 節の規則で merge する。
- Implementer は push・merge・PR の作成をしない。`main` に触れない。
- Orchestrator は 1.3 節と 1.4 節の時点でだけ `git push origin refactor/kashiwade/native-diagrams` する（force push は禁止）。PR は作らない。`main` に merge しない（人間が PR で行う）。

### 0.2 禁止事項（全 unit 共通）

1. 互換のためのコードを書かない。旧名の alias、`#[deprecated]`、wrapper、旧と新の二重の経路を作らない。旧名は消し、呼出し元を全部直す。
2. CI の設定・workflow を足さない（付属書 V の V1 が作る `viewer-windows.yml` を除く）。
3. 付属書が名前を挙げていない機能の試験を足さない。ただし、その unit の変更が壊しうる不変条件を確かめる最小の試験は、その unit が所有する試験のファイルに足してよい。期待値を緩めない（許容差を広げる、assert を消す、`#[ignore]` を付ける）。
4. 既存の試験の期待値を変えない。例外は、付属書の「既存の試験の移行」の表に挙げたものだけ（設計書 6.4 節の 2）。
5. `#[allow(..)]`・`#[expect(..)]` を、警告の件数を減らす目的で足さない。
6. 1 関数 80 code 行、1 ファイル 1000 code 行、引数 7 個の上限を超えない。超えそうなら意味のある helper に分ける。
7. 実装の途中で `cargo test --workspace`、clippy を起動する style の試験（`xtask-style/tests/tools.rs`）、基準の全件の試験（`tests/baseline.rs`）、viewer の GUI の試験（`tests/viewer.rs`）を実行しない。unit で実行してよいのは 0.6 節と、付属書の unit の「確認」のコマンドだけ（V3 は GUI の試験を 1 回だけ実行する）。
8. 依存の crate を足さない（設計書か付属書が名前を挙げたものを除く）。
9. 出力の順に影響する場所で `HashMap`・`HashSet` と乱数を使わない。
10. **行番号を信じない。** 付属書と設計書 7 章の `file:line` は、基点 `e7eb600` のもの。関数名・型名・field 名・周りのコードで場所を探す。
11. 設計書の 5 章（公開の契約）と違うコードを書かない。付属書の手順が契約と食い違うなら STOP する。
12. 所有ファイル（付属書の unit の「所有」と、4 章の表）の外を変えない。署名を変えた結果の「compile を通すための機械的な追従」は、同じ crate の中で、**同じ wave の他の lane が所有しないファイル**に限って行ってよい。他の lane の所有のファイルか、他の crate なら STOP する。
13. 生成物（`target/`、`viewer/target/`、`architecture/`、`*.xtaskmap`）を commit しない。root の `Cargo.lock` と `viewer/Cargo.lock` は commit してよいが、merge のときに Orchestrator が作り直す（1.3 節）。
14. **旧い実装は `git show e7eb600:<path>` で読む。** 作業ツリーの旧いファイルは、別の unit が消していることがあるので頼らない。
15. 新しく作る `.rs` のファイルは、すべて 1 行目を `//! Role: <そのファイルの役割を 1 行、80 字以内>` にする（設計書 6.3 節。xtask 自身の検査が要求する）。付属書がコード片に Role の行を書いていれば、その文を使う。

### 0.3 unit の読み方

付属書の各 unit は、所有、目的、手順、試験、確認の欄を持つ。

- 手順は番号の順に行う。
- 「このとおりに書く」と書いたコード片は、一字一句そのまま書く（`use` の過不足は compile error に従って直してよい）。
- 「試験」の欄の試験を先に書き、失敗することを確かめてから実装する（試験の対象が無いことによる compile error も失敗とみなす）。
- **優先順位**（設計書 0 章と同じ）：設計書 5 章（契約）＞ 本書 4 章の「追加の指示」＞ 付属書 ＞ 本書の共通の規則。付属書か追加の指示が設計書 5 章と食い違うときは STOP する。

### 0.4 停止して報告（STOP）

次の場合は、その項目の作業を止め、他の項目を続け、最後の報告に `STOP:` で始まる行を書く。

- 付属書の手順が実際のコードと合わない（関数・型・field が無い、型が違う、名前で探しても見つからない）。
- 手順を行うと、所有ファイルの外を変える必要がある（0.2 の 12 の追従を除く）。
- 同じ compile error を 3 回直しても消えない。
- 付属書の試験の期待値が、手順どおりに実装しても満たせない。期待値は変えない。
- 既存の試験が、移行の表に無いのに通らなくなった。
- 付属書のアルゴリズムの説明で、手順が一つに決まらない（二通り以上に読める）。
- 設計書 5 章の契約と、付属書の手順が食い違う。
- Kivact の API が付属書 V の 0 章の表と違う（`<KIVACT>`（1.1 節）のソースで確かめた上で）。

STOP にせず unit の中で直すもの（設計の判断ではない）：

- 関数・ファイル・引数の上限を超えた（意味のある helper に分ける。引数は struct にまとめる）。
- clippy の機械的な指摘（clippy の提案どおりに直す）。
- rustfmt、`use` の過不足。

書式：`STOP: <unit> <file>:<関数名> <何が合わないか 1 文> / <試したこと 1 文>`。

STOP した項目があっても、残りの項目は続け、compile が通る状態で commit する。STOP した項目は `todo!("STOP <unit>")` の本体のまま残してよい。

### 0.5 Orchestrator の既定の判断表

Orchestrator は設計の判断をしない。`STOP:` に次の表が当たればその判断をする。当たらなければ、その項目を**設計者への差戻し**として `rework/OPEN.md`（無ければ作る）の「差戻し」節に 1 行で書き（unit、場所、STOP の文、付属書の節）、1.4 節の規則で進める。

| 状況 | 判断 |
|---|---|
| 行番号がずれているが、同じ名前の関数・型・field が近くにある | その名前に対して手順を行うよう、同じ subagent に指示する（SendMessage） |
| 所有ファイルの外の 1〜3 ファイルを変える必要がある | 同じ wave の他の lane の所有でなければ許可する。他の lane の所有なら、その項目を次の wave に回す |
| compile error が、同じ wave の他の lane の未 merge の変更を前提にしている | その unit の確認の失敗を記録し、merge の後の tier b で確かめる。tier b でも失敗すれば、その lane の fix にする（1.3 節） |
| subagent が確認のコマンドを実行していない | 同じ subagent に実行させる |
| 試験が期待値に届かない（配置の試験を含む） | 差戻し。期待値を緩めない |
| KPI（設計書 2 章）が目標に届かない | 差戻し |
| network が無く、viewer の依存を取得できない | V の lane を止め、人間に報告する |

### 0.6 unit で実行してよいコマンド

`<ROOT>` は unit の worktree の絶対 path。viewer のコマンドは必ず subshell `( .. )` の中で `cd` する（後のコマンドの directory を変えないため）。

```bash
cargo fmt -p <crate>                                            # 必ず最後に
cargo check -p <crate> --all-targets                            # 何度でも
cargo clippy -p <crate> --all-targets -- -D warnings            # 付属書の「確認」が挙げたときだけ
cargo test -p <crate> <filter>                                  # 付属書の「確認」が挙げた範囲だけ
(cd <ROOT>/viewer && cargo check -p <crate> --all-targets)      # viewer の lane
(cd <ROOT>/viewer && cargo test -p <crate> --lib)               # viewer の lane（付属書の「確認」が挙げたとき）
```

- すべての cargo のコマンドに、環境変数 `CARGO_TARGET_DIR=<1.1 節の target の絶対 path>` を付ける。
- **骨組みの警告。** 付属書が `todo!()` の骨組みと明記したファイル（付属書 L の 1.4 節、付属書 V の V1 の手順 6）に由来する `dead_code`・`unused_variables` の警告は、その本体を書く unit が終わるまで残してよい。
  - その間、該当する crate には `clippy -D warnings` を実行しない。付属書の「確認」の欄に clippy があるのは、crate が完成した unit だけ。
  - 引数の名前を `_` で始めて警告を消すことはしない（署名が変わるため）。
  - それ以外の警告は直す。
- `cargo test` の filter が 0 件の試験しか実行しなかったら、失敗として扱う（出力の `running 0 tests` を見る）。
- W1〜W4 の間、`xtask-core` は compile できない（設計書 9 章）。`cargo check --workspace` は実行しない。

---

## 1. 体制

### 1.1 branch・worktree・target・Kivact

- 統合の branch：`refactor/kashiwade/native-diagrams`。W0 で作る。Orchestrator は、統合の checkout（このリポジトリの元の directory。以下 `<MAIN>`）で merge と gate を行う。
- `<WT>` は `<MAIN>/../xtask-core-wt` の絶対 path。W0 で一度だけ求め、以後は文字どおりの絶対 path を使う（worktree の中で相対 path を解釈し直さない）。
- **lane の worktree は wave ごとに作る。** wave `<W>` の lane `<L>`：

  ```bash
  git -C <MAIN> worktree add <WT>/W<W>-<L> -b wt/W<W>-<L> refactor/kashiwade/native-diagrams
  ```

  その lane の unit は、すべて同じ worktree で一つずつ行う（unit ごとに commit）。
- **target。**
  - xtask の crate は lane ごとに `<WT>/target-<L>`（L は 1〜6）を使う。
  - viewer の crate は、Kivact の build が大きいため、lane をまたいで `<WT>/target-viewer` を共有する。
  - 同じ target を使う 2 つの lane の build は、cargo の lock で順になる。これは正常。
- **Kivact のソース。** `<KIVACT>` は `<WT>/kivact`。W0 で固定の commit を clone する（3 章）。付属書 V が Kivact の文書・ソースを参照するときは、ここのファイルを直接読む。

### 1.2 subagent への指示の雛形

Orchestrator は各 unit に次の文をそのまま渡し、`<..>` だけを埋める。

```text
あなたは xtask-core の改修の unit <unit ID> の実装者です。あなた自身は subagent を作らないでください。
作業場所は <worktree の絶対 path> です。ソースの編集はこの directory の中だけで行ってください。
build の出力は CARGO_TARGET_DIR（下記）と cargo の cache に出てよく、それは編集とはみなしません。
すべての cargo のコマンドに次の環境変数を付けてください：CARGO_TARGET_DIR=<target の絶対 path>
Kivact のソースは <KIVACT の絶対 path> にあります（viewer の unit だけが使います）。
最初に次を読んでください：
  1. <worktree>/REWORK_WORK_ORDER.md の 0 章（規則）と、4 章の表の「<unit ID>」の行（追加の指示を含む）
  2. <worktree>/rework/<付属書のファイル> の、<unit ID> の節と、その節が参照する節
  3. <worktree>/REWORK_DESIGN.md の 5 章と 6 章のうち、その節が参照する箇所
付属書の手順を番号の順に実行してください。「試験」の欄の試験を先に書いてください。行番号はずれています。関数名・型名・field 名で場所を探してください。
旧い実装は git -C <worktree> show e7eb600:<path> で読んでください。
優先順位は 設計書 5 章 ＞ 作業指示書 4 章の追加の指示 ＞ 付属書 です。付属書か追加の指示が設計書 5 章と食い違うときは STOP してください。
設計の判断が必要になったら、その項目の作業を止め、0.4 節の書式で STOP: 行を書き、残りの項目を続けてください。
付属書の「確認」のコマンドをすべて実行し、error と警告を直してください。最後に cargo fmt -p <変更した各 crate> を実行してください。
最後に `git -C <worktree> add -A && git -C <worktree> commit -m "<unit ID>: <要約>"` で commit してください。push・merge はしないでください。
報告には次だけを書いてください：変更した file の一覧、確認の各コマンドの結果（成功/失敗と最後の数行）、試験の各項目の結果、STOP: 行。
```

### 1.3 merge の手順（Orchestrator、各 wave の終わり）

4 章の表の lane の順に、次を実行する。

```bash
git -C <MAIN> checkout refactor/kashiwade/native-diagrams
git -C <MAIN> merge --no-ff wt/W<W>-<L> -m "rework W<W>-<L>: <unit ID の列>"
```

- **conflict：**
  - root の `Cargo.toml` の `members`・`[workspace.dependencies]` の行、`dev/xtask/policy/workspace.toml` の行、`lib.rs` の `mod`・`pub use` の行は、両方の行を残し、アルファベット順に並べる。
  - `Cargo.lock` と `viewer/Cargo.lock` は、`git checkout --theirs <lock>` の後に作り直す：root は `cargo metadata --format-version 1 --offline > /dev/null`、viewer は `(cd <MAIN>/viewer && cargo metadata --format-version 1 --offline > /dev/null)`。
  - それ以外は、4 章の表でそのファイルを所有する lane の版を採る。同じ wave で二つの lane が同じファイルを持たないように表を作ってある。それでも起きたら作業を止め、人間に報告する。
- 全 lane を merge したら、lock を作り直して変化があれば `git commit -am "rework W<W>: refresh lock files"` する。その後、2 章の **tier b** を 1 回実行する。
  - 失敗したら、error を出した crate を所有した lane の最後の unit の subagent に、error の全文を渡して直させる。
  - 直しは、統合の branch の今の HEAD から作った worktree `<WT>/W<W>-<L>-fix`（branch `wt/W<W>-<L>-fix`）で行い、同じ手順で merge し、tier b をもう一度実行する。
- merge の後、worktree と branch を消す：`git -C <MAIN> worktree remove <WT>/W<W>-<L> && git -C <MAIN> branch -D wt/W<W>-<L>`。
- `rework/PROGRESS.md`（無ければ作る）に、unit ごとの状態（1.4 節）と `W<W> merged <commit>: tier b 成功|失敗` の行を書き、**commit してから** push する：

  ```bash
  git -C <MAIN> add rework/PROGRESS.md rework/OPEN.md 2>/dev/null; git -C <MAIN> commit -m "rework W<W>: progress"
  git -C <MAIN> push origin refactor/kashiwade/native-diagrams
  ```

### 1.4 進行の規則

unit の状態は次の四つで、`rework/PROGRESS.md` の表に書く。

| 状態 | 意味 |
|---|---|
| 未着手 | subagent を起動していない |
| 実装中 | subagent が動いている |
| 統合済み | merge し、その wave の tier b が成功した。STOP が無い |
| 差戻し | STOP が差戻しになった（`rework/OPEN.md` に行がある）。STOP の無い部分は merge してよい |

- 次の wave の unit を起動してよいのは、その unit の「前提」（5 章）の unit がすべて**統合済み**のときだけ。前提に差戻しがある unit は起動せず、他を進める。
- wave の tier b が失敗のまま次の wave に進まない。
- 差戻しは、設計者が付属書を直した後に、Orchestrator がその unit の STOP の項目だけを行う fix の lane（1.3 節の fix と同じ手順）で解消する。merge の後に、その wave の tier b をもう一度実行し、状態を統合済みにする。その unit を前提とする unit が既に統合済みなら、その unit の確認のコマンドも実行し直す。
- 差戻しが 1 件でも残っていれば、CP で完了を宣言しない。

---

## 2. gate の 3 段

### 2.1 tier a（unit ごと、Implementer）

付属書の各 unit の「確認」のコマンド。

### 2.2 tier b（wave の merge の後、Orchestrator、`<MAIN>` で実行）

`CARGO_TARGET_DIR` は `<WT>/target-main`（viewer は `<WT>/target-viewer`）にする。

| wave | コマンド |
|---|---|
| W1 | `cargo check -p xtask-layout -p xtask-archive -p xtask-policy -p xtask-workspace -p xtask-source --all-targets`、`cargo test -p xtask-archive -p xtask-policy -p xtask-workspace -p xtask-source`、`cargo test -p xtask-layout --lib` |
| W2 | W1 のもの、`cargo check -p xtask-modules --all-targets`、`cargo test -p xtask-modules`、`cargo clippy -p xtask-archive -p xtask-policy -p xtask-modules --all-targets -- -D warnings`、`(cd <MAIN>/viewer && cargo check --workspace --all-targets && cargo test -p xtask-render)` |
| W3 | W2 のもの、`cargo test -p xtask-layout --lib`、`cargo clippy -p xtask-layout --all-targets -- -D warnings`、`cargo test -p xtask-layout --release --test baseline`、`cargo run -p xtask-layout --release --example baseline_report`（K1〜K4 を `rework/PROGRESS.md` に記録。未達なら差戻しにして進める）、`cargo check -p xtask-style --all-targets`、`cargo test -p xtask-style --lib`、`(cd <MAIN>/viewer && cargo check --workspace --all-targets && cargo test -p xtask-viewer --lib)` |
| W4 | W3 のもの（基準の試験と比較表を除く）、`cargo check -p xtask-diagram --all-targets`、`cargo test -p xtask-diagram`。viewer の GUI の試験は V3 の報告の結果を記録するだけで、再実行しない |
| W5 | `cargo check --workspace --all-targets`、`cargo test -p xtask-core --lib`、`cargo test -p xtask-core --test integration` |

### 2.3 tier c（CP、Orchestrator）

`<MAIN>` の絶対 path で実行する。viewer のコマンドは subshell に入れる。

```bash
cd <MAIN>
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo +1.85.0 check --workspace --all-targets --locked             # Rust 1.85 の契約（AC-6）
cargo test --workspace                                              # xtask-style の tests/tools.rs（clippy を起動する試験）を含む
cargo test -p xtask-layout --release --test baseline
cargo run -p xtask-layout --release --example baseline_report       # K1〜K4 を記録
cargo run -p xtask-core -- check                                    # 違反 0 件
cargo run -p xtask-core -- check --root <MAIN>/viewer               # viewer 自身の検査
(cd <MAIN>/viewer && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && KIVACT_TEST_MIRROR=off cargo test --workspace)
git -C <MAIN> ls-files | grep -E '\.(js|cjs|html|css)$'             # 何も出ないこと（AC-1）
```

**node の無い PATH で `--write`（AC-1）。** Git Bash で、次のとおりに実行する。

```bash
cargo build -p xtask-core                                           # 先に build する（PATH を変える前）
NODE_FREE_PATH="$(dirname "$(command -v cargo)"):/usr/bin:/c/Windows/System32"
PATH="$NODE_FREE_PATH" bash -c 'command -v node' && { echo "node is still reachable"; exit 1; }
PATH="$NODE_FREE_PATH" cargo run -p xtask-core -- check --write     # K5 を記録
```

- 結果と KPI の値を `rework/PROGRESS.md` に書く。
- K6 は 2.4 節の手順で測る。
- 設計書 7 章の「対応」の所見と、`rework/R-review-1.md` の「採用」の指摘を、「確かめ方」の欄の証拠で一件ずつ点検する。証拠の無いものを差戻しにする（AC-5）。
- 記録は検証した commit の hash とともに `rework/PROGRESS.md` に書き、別の commit にして push する。

### 2.4 K6 の測り方

同じ機械、同じ toolchain、同じ `CARGO_TARGET_DIR`（`<WT>/target-main`）で行う。

```bash
cargo build -p xtask-core                                           # build の時間は含めない
cargo run -p xtask-core -- check > /dev/null 2>&1                   # 暖機（1 回、測らない）
for i in 1 2 3; do /usr/bin/time -f %e cargo run -p xtask-core -- check > /dev/null 2>> <WT>/k6.txt; done
```

- 3 回の中央値を記録する。
- 改修前（W0）は、`check` の代わりに `check --no-write` で同じ手順を行う。
- 判定は「改修後の中央値 ≤ 改修前の中央値 × 1.05」。
- `/usr/bin/time` が無い環境では、bash の `TIMEFORMAT=%R; time ...` を使う。

---

## 3. W0：準備（Orchestrator が自分で行う）

1. 設計書・作業指示書・付属書が commit 済みであることを確かめる。
2. `git -C <MAIN> checkout -b refactor/kashiwade/native-diagrams` で統合の branch を作る（今の HEAD から）。
3. `<WT>` を作り、その絶対 path を `rework/PROGRESS.md` の先頭に書く。
4. Kivact を固定の commit で clone する（network が要る。無ければ人間に報告して止める）：

   ```bash
   git clone https://github.com/Kashiwade-music/kivact <WT>/kivact
   git -C <WT>/kivact checkout --detach 2b8df5aceea40c9ae23950f9d6c942559c89e840
   ```

5. Rust 1.85 を入れる：`rustup toolchain install 1.85.0 --profile minimal`（入っていれば何もしない）。
6. 改修前の K6 の基準を 2.4 節の手順で測り、`rework/PROGRESS.md` に書く。
7. 5 章の全 unit を「未着手」で `rework/PROGRESS.md` の表に書き、commit して `git -C <MAIN> push -u origin refactor/kashiwade/native-diagrams` する。

---

## 4. wave の表

所有のファイルは付属書の各 unit の「所有」の欄。下の表の「追加の指示」は付属書より優先する。lane の欄の `→` は、同じ lane で順に行う unit。

### W1

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L1 | L 1.2〜1.4 節、2 章、6 章 | `xtask-layout` の旧いファイル（1.2 節の「消すもの」）を消す。1.4 節の骨組み（全ファイルの署名と `lib.rs` の完成形）を作る。後の unit は `lib.rs` を変えない |
| 2 | A1 | A 1 章 | root の `Cargo.toml` の `[workspace.dependencies]` に 2 行を足すのは、この unit |
| 3 | M1 | M の M1 | `xtask_source::normalize` は M2 が作る。compile が通らなければ報告に書く（STOP にしない） |
| 4 | M2 | M の M2 | — |

### W2

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L2 → L6 | L 3 章、4 章 → L 10 章 | L2 は `layering.rs` と `chains.rs` の両方。L6 の試験の Layout は手で作る（`layout` を呼ばない） |
| 2 | L3 | L 5 章 | 試験の Chains は手で作る |
| 3 | L4 | L 7 章 | 試験の Chains と Order は手で作る |
| 4 | L5 | L 8 章 | 試験の入力は手で作る |
| 5 | M3a → M3b → M3c | M の M3a、M3b、M3c | `dev/xtask/policy/workspace.toml` は `trusted_macros` の行だけを変える（M3c） |
| 6 | V1 | V の V1 | network が要る。target は `target-viewer` |

### W3

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | L7 | L 9 章 | `hierarchy.rs` と `level.rs` の `run_level` の本体。`lib.rs` は変えない |
| 2 | L8 | L 11 章 | `tests/baseline.rs` は compile だけ確かめる（`cargo test -p xtask-layout --release --test baseline --no-run`）。実行は tier b |
| 3 | C1 | C の C1 | `xtask-core` からファイルを移すだけで、xtask-core 側の compile は直さない（W5 の C2 が直す） |
| 4 | V2a | V の V2a | target は `target-viewer` |
| 5 | V2b → V2d | V の V2b → V2d | target は `target-viewer` |
| 6 | V2c | V の V2c | target は `target-viewer` |

### W4

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | A2 | A 2 章 | — |
| 2 | V3 | V の V3 | GUI の試験は、すべて書き終えてから 1 回だけ実行する。失敗したら原因を直し、もう 1 回だけ実行してよい |
| 3 | C3 | C の C3 | 設計書 3 章と 5.1 節に従う。実装の結果ではなく設計書を正とする |

### W5

| lane | unit | 付属書 | 追加の指示 |
|---|---|---|---|
| 1 | C2a → C2b → C2c | C の C2a、C2b、C2c | 確認の統合の試験は、付属書が挙げた試験だけを実行する |

### CP（W5 の後）

2.3 節の tier c。差戻しが 0 件で、AC-1〜AC-7 と KPI を満たせば完了。`rework/PROGRESS.md` に `CP passed <commit>` を書いて commit し、push し、人間に PR の作成を依頼する。

---

## 5. unit の索引

| unit | wave | lane | 付属書 | 前提 |
|---|---|---|---|---|
| L1 | W1 | 1 | L | — |
| A1 | W1 | 2 | A | — |
| M1 | W1 | 3 | M | — |
| M2 | W1 | 4 | M | — |
| L2 | W2 | 1 | L | L1 |
| L6 | W2 | 1（L2 の後） | L | L1 |
| L3 | W2 | 2 | L | L1 |
| L4 | W2 | 3 | L | L1 |
| L5 | W2 | 4 | L | L1 |
| M3a | W2 | 5 | M | M1、M2 |
| M3b | W2 | 5（M3a の後） | M | M3a |
| M3c | W2 | 5（M3b の後） | M | M3b |
| V1 | W2 | 6 | V | A1 |
| L7 | W3 | 1 | L | L2〜L6 |
| L8 | W3 | 2 | L | L1 |
| C1 | W3 | 3 | C | M1、M2 |
| V2a | W3 | 4 | V | V1 |
| V2b | W3 | 5 | V | V1 |
| V2d | W3 | 5（V2b の後） | V | V2b |
| V2c | W3 | 6 | V | V1 |
| A2 | W4 | 1 | A | A1、L7、M1、M3c |
| V3 | W4 | 2 | V | V2a〜V2d |
| C3 | W4 | 3 | C | — |
| C2a | W5 | 1 | C | すべて |
| C2b | W5 | 1（C2a の後） | C | C2a |
| C2c | W5 | 1（C2b の後） | C | C2b |
