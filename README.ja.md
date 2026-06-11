<p align="center">
  <img src="assets/screenshot.png" alt="Zagens スクリーンショット" width="800" />
</p>

# Zagens — デスクトップ Agent ハーネス

**[English](README.md)** · **[中文](README.zh-CN.md)** · **[Português (BR)](README.pt-BR.md)** | 日本語

長時間の Agent 作業は**途中で止まったり、早すぎる「完了」宣言**をしがちです。コードと Office ファイルは**別ツール**に分かれがちです。ローカル Agent には、チャット窓だけでなく**リプレイ・承認・監査可能性**が必要です。

**Zagens** は **[DeepSeek V4](https://deepseek.com/) エコシステム**向けに設計した**デスクトップ Agent ハーネス**です。DeepSeek API・推論ストリーム・ツール呼び出しに最適化され、既定で DeepSeek Pro / Flash を利用できます。Code / Office ワークスペースで共通のローカル **runtime sidecar**、ターン単位の**セッションリプレイ**、長時間タスク向けの**段階的完了ゲート**、トレイ・通知・組み込みターミナルなどのデスクトップネイティブ機能を備えます（OpenAI 互換エンドポイントもフォールバックとして利用可能）。

> **作者より：** AI Agent が何でもできるわけではない — 境界がある。私たちにできるのは、その境界を広げることだ。

> **ライセンス:** [MIT](LICENSE)。Runtime 系譜: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/)。以下は **Zagens v0.7.4** 時点 — [CHANGELOG.md](CHANGELOG.md) を参照。

| リソース | リンク |
|----------|--------|
| ユーザーガイド | [zagens.com/docs](https://zagens.com/docs) |
| ダウンロード | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)（最新 **`zagens-v0.7.4`**）· [zagens.com/download](https://zagens.com/download) |
| 設計仕様 | [`docs/README.md`](docs/README.md) |
| コントリビューション | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| セキュリティ | [`SECURITY.md`](SECURITY.md) |

---

## 向いている人 / 向いていない人

| 向いている | 向いていない |
|------------|--------------|
| **DeepSeek ヘビーユーザー** — DeepSeek API / V4 で日々コーディング Agent を回し、公式 TUI より強いデスクトップ Harness を求める人 | モデル・課金込みのホスト型 SaaS |
| **独立したデスクトップ Harness**（特定 IDE 拡張に縛られない）を求める開発者 | ツール・ワークスペース・リプレイのない「チャットのみ」 |
| **長期コードリファクタ**や **Office 成果物**を同一ワークフローで扱うチーム | ガードレールなしの完全自律 YOLO Agent |
| **ローカル sidecar**、MCP/スキル、UI 内**実行承認**を重視する人 | セットアップ不要のモバイル / ブラウザのみ |
| 現時点では **Windows デスクトップ**；macOS/Linux は **CLI** またはソースビルド | ローカル実行なしの Web コパイロットだけで足りるチーム |

---

## Zagens を定義する 3 点

**1. チャットシェルではなく Harness** — 長時間コードタスクは** composable 完了ゲート**（オペレータ / モデル / ツールチェーン）で判定。「モデルが終わったと言った」だけでは完了にしません。仕様: [LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · フィクスチャ: [`fixtures/harness/`](fixtures/harness/)。

**2. デスクトップネイティブな制御面** — [Tauri 2](https://tauri.app/) UI + loopback **sidecar**（`zagens-runtime`）：トレイ、通知、diff、**セッションリプレイ**、Code ワークスペース **PTY**、HTTP ツール承認。ヘッドレス **`zagens`** CLI と同一エンジン。

**3. Code + Office、1 つの runtime** — **Code / Office** タスク種別は設定とツールを共有しつつ、プロンプトとツール面は異なります。種別切替は KV 安定のため**新セッション**を開始（[アーキテクチャ](docs/task-type-prompt-architecture.md)）。Office: `read_file` / **`write_office`**（xlsx は Rust、docx/pptx/pdf は同梱 Python）。

その他: **CRAFT マルチエージェント**（サブエージェント、fix-loop 判定、P1 ブラックボード — [メモ](docs/craft-v2-improvements.md)）、遅延**シンボル索引**（`.zagens/symbols.json`）、MCP、スキル、Hooks、スケジュールタスク。

---

## 重点課題

| 課題 | Zagens のアプローチ |
|------|---------------------|
| Agent が途中停止、または早すぎる完了マーク | **段階的完了ゲート** + 長時間タスクパネル（[composable harness](docs/harness/COMPOSABLE_HARNESS.md)） |
| IDE プラグインとターミナル Agent のセッション分断 | 単一 **sidecar** + SQLite スレッド、fork/再開、**リプレイ**、ワークスペーススナップショット |
| 表計算・文書がコーディング Agent の外 | **Office モード** + `write_office` + デスクトッププレビュー |
| ローカルツール実行を盲信しない | 実行ポリシー、ネットワーク規則、パス正規化、承認 UI、runtime トークンは WebView に入れない（[サンドボックス行列](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)） |

---

## 現時点で提供（v0.7.4）

**デスクトップ:** マルチセッションチャット（ストリーム/停止/思考）、ファイルツリー・プレビュー・diff、PTY ターミナル（Code）、サブエージェントパネル、タスク/スキル UI、MCP + ルーティング、使用量チャート、keyring API キー、UI: 簡体中文 / en / ja / pt-BR。

**Runtime:** スレッド、MCP、スキル（`~/.zagens/skills`）、ライフサイクル Hooks、マルチプロバイダルーティング、ビジョン（`describe_image`）。

**ツール（代表）:** ファイル（`read_file`、`write_file`、`edit_file` …）、git、`exec_shell`、`write_office`、任意で `web_search` / `fetch_url`、メモリツール。一覧: `crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md)。

---

## 既知の制限（依存前に）

マーケ用チェックリストより、正直なスコープを優先します。

| 項目 | 状態 |
|------|------|
| **デスクトップインストーラ** | **Windows** は [Releases](https://github.com/didclawapp-ai/zagens/releases)。**macOS / Linux デスクトップパッケージ** — 計画中。3 プラットフォーム **CLI** は提供済み。 |
| **OS サンドボックス強制** | **macOS Seatbelt** — `sandbox-exec` 利用時に強制。**Windows** — ネイティブサンドボックス実装済み（`elevated` 推奨：`zagens sandbox setup` 後に強制；`unelevated` は workspace 書き込み隔離のみ）。設定 → **Sandbox** 初回ウィザード。**Linux** — ポリシー宣言のみ、**OS 未強制**（degraded）。詳細: [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)。 |
| **プロバイダ** | **DeepSeek V4**（Pro / Flash）向けに最適化。API キーはユーザー提供。OpenAI 互換エンドポイントも利用可 — **モデルはホストしません**。 |
| **長時間 & マルチエージェント** | ゲートと CRAFT は**利用可能だが進化中**；エッジケースと新ゲート種別を開発中。 |
| **Office の深さ** | コア読み書きは動作；エンタープライズコネクタ、音声、一部シナリオテンプレは**将来**（[Office シナリオ](docs/desktop/OFFICE_SCENARIOS.md)）。 |

セキュリティ報告: [`SECURITY.md`](SECURITY.md)。

---

## 今後の方向

公開設計仕様: [`docs/`](docs/README.md)。方向性:

- **プラットフォーム parity** — macOS/Linux デスクトップインストーラ；**Linux** ネイティブサンドボックス（Landlock/bwrap）。Windows ネイティブサンドボックスは 0.7.x で提供済み。
- **信頼できる長時間タスク** — より厳密な完了ゲート、Harness フィクスチャ、リプレイ可能なオペレータワークフロー。
- **Office ワークフロー** — 共有 runtime から分離せずシナリオを拡充。
- **ハードニング** — [CHANGELOG](CHANGELOG.md) と [SECURITY.md](SECURITY.md) で追跡。

---

## クイックスタート

**ビルド済み（Windows）:** [GitHub Releases `zagens-v0.7.3`](https://github.com/didclawapp-ai/zagens/releases) — インストーラ zip + CLI。SmartScreen: [SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md)。

**ソースから:**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # sidecar を crates/desktop/binaries/ にコピー

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API キー: Zagens 設定、または ~/.zagens/config.toml
```

**ヘッドレス CLI**（デスクトップと同一 runtime）:

```bash
cargo install zagens-cli --version 0.7.3 --bin zagens --locked

zagens doctor
zagens exec 'summarize src/' --json
zagens exec 'refactor auth module' --auto
zagens serve --http --port 7878
```

ビルド済み CLI + SHA-256: [Releases](https://github.com/didclawapp-ai/zagens/releases)。設定: [config.example.toml](config.example.toml)。

---

## アーキテクチャ

```
┌──────────────────────────────────────────────────────────────┐
│                     Zagens (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust Shell                │  │
│  │   React / TS    │◄─┤  commands, sidecar supervisor,    │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │                       │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │     Runtime API (embedded sidecar, loopback HTTP/SSE)    │  │
│  │  /v1/threads, /v1/skills, /v1/symbol-index, ...         │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Shared crates: agent, core, config, state, tools, mcp   │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

境界: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · HTTP: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。

### セキュリティモード（`sandbox_mode`）

| モード | 説明 |
|--------|------|
| `read-only` | Shell 実行・ファイル書き込みなし |
| `workspace-write` | Shell と書き込みはワークスペース内（推奨デフォルト） |
| `danger-full-access` | フルファイルシステム — 注意して使用 |
| `external-sandbox` | `exec_shell` を OpenSandbox 互換 API へ |

承認ポリシー（`on-request` / `untrusted` / `never`）、ドメイン別ネットワーク規則、OS keyring。runtime トークンは WebView に入りません。

---

## 開発

**前提:** Rust 1.88+（MSRV；CI **1.96**）、Node.js 20 LTS、Python 3.8+、[Tauri CLI 2](https://v2.tauri.app/start/prerequisites/)。

**[CONTRIBUTING.md](CONTRIBUTING.md)** · **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)**。

| コマンド | 説明 |
|---------|------|
| `bash scripts/ci/verify-lint.sh` | CI lint ミラー |
| `bash scripts/ci/verify-workspace.sh` | lint + 全 workspace テスト |
| `cargo test --workspace --all-features` | 全テスト |
| `cd crates/desktop && cargo tauri dev` | デスクトップ開発起動 |

Windows: `pwsh -File scripts/ci/verify-lint.ps1`

```
zagens/
├── crates/desktop/       # Tauri アプリ
├── crates/runtime-server/ # Sidecar HTTP/SSE
├── docs/                 # 公開設計仕様
├── fixtures/harness/     # LHT / Office フィクスチャ
└── config.example.toml
```

---

## ライセンス

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors。追加帰属: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE)。
