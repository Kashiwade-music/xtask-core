# ELK配置の比較基準

Node.jsとelkjsを自前の配置エンジンに置き換える前に、ELKが出した配置結果を記録したもの。
新エンジンとの比較に使う。比較する指標は交差数・折れ数・面積など。
見た目をELKと一致させることは目標にしない。

## 内容

1つの図につき1ファイル。JSONを1行で書いている。

```json
{"input": { ... }, "scene": { ... }}
```

- `input`：`xtask-layout`が`runner.cjs`へ送った要求（`prepare.rs`の`Prepared`）。文字の折り返しと寸法は計算済み。
- `scene`：ELKの結果を`runner.cjs`が変換したもの。`xtask_layout::Scene`と同じ形。

ファイル名は`<package>.<target>.<kind>.<normal|test>.json`。crate依存図は`crates.json`。

| ディレクトリ | 対象 | 図の数 |
| --- | --- | --- |
| `kivact/` | Kivact `c8cec8057013fe149139c7e585054adb4976de8e`（2026-11-18）。同梱xtask-coreは`1aeabaf`、`[diagrams] targets = "lib-bin"` | 121 |
| `xtask-core/` | このリポジトリ `ca813dc2ce2013dbea6dfeeff9b090ec36b589e0` | 25 |

両commitとも、配置関連のコード（`xtask-layout`・`xtask-svg`・`xtask-graph`）は同一。
elkjs 0.12.0、Node.js v26.13.1、2026-11-18に取得。
xtask-coreで2回取得し、全図の結果が完全に一致することを確認した（ELKは`randomSeed = 1`で決定的）。

## 取得方法

Node.js と elkjs を使って取得した。改修で Node.js を削除したため、再取得の手段は無い。取得に使った script は commit `e7eb600` にある。
