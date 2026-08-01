<p align="center">
  <img src="assets/screenshot.gif" alt="Zagens スクリーンショット" width="800" />
</p>

# Zagens — DeepSeek V4 向けオープンソース Agent harness

**[English](README.md)** · **[中文](README.zh-CN.md)** · **[Português (BR)](README.pt-BR.md)** | 日本語

長時間の Agent 作業は**途中で止まったり、早すぎる「完了」宣言**をしがちです。コードと Office ファイルは**別ツール**に分かれがちです。ローカル Agent には、チャット窓だけでなく**リプレイ・承認・監査可能性**が必要です。

**Zagens** は **[DeepSeek V4](https://deepseek.com/)** 向けのオープンソース Agent harness です。

> **作者より：** AI Agent が何でもできるわけではない — 境界がある。私たちにできるのは、その境界を広げることだ。

> **ライセンス:** [MIT](LICENSE)。Runtime 系譜: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/)。以下は **Zagens v0.9.0** 時点 — [CHANGELOG.md](CHANGELOG.md) を参照。

| リソース | リンク |
|----------|--------|
| ユーザーガイド | [zagens.com/docs](https://zagens.com/docs) |
| ダウンロード | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases)（最新 **`zagens-v0.9.0`**）· [zagens.com/download](https://zagens.com/download) |
| 設計仕様 | [`docs/README.md`](docs/README.md) |
| コントリビューション | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| セキュリティ | [`SECURITY.md`](SECURITY.md) |

---

## 向いている人 / 向いていない人

| 向いている | 向いていない |
|------------|--------------|
| **DeepSeek ヘビーユーザー** — DeepSeek API / V4 で日々コーディング Agent を回し、公式ツールを超えるローカル Agent プラットフォームを求める人 | モデル・課金込みのホスト型 SaaS |
| **独立した Agent プラットフォーム**（デスクトップ / TUI / CLI、特定 IDE 拡張に縛られない）を求める開発者 | ツール・ワークスペース・リプレイのない「チャットのみ」 |
| **ターミナル優先**ユーザー（macOS / Linux / Windows）— 全画面 **`zagens-tui`**、デスクトップと同一エンジン | ガードレールなしの完全自律 YOLO Agent |
| **長期コードリファクタ**や **Office 成果物**を同一ワークフローで扱うチーム | セットアップ不要のモバイル / ブラウザのみ |
| **ローカル sidecar**、MCP/スキル、UI 内**実行承認**を重視する人 | ローカル実行なしの Web コパイロットだけで足りるチーム |
| 現時点では **Windows デスクトップ**；macOS/Linux は **TUI**、**CLI** またはソースビルド | |

---

## Zagens を定義する 3 点

**1. チャットシェルではなく Harness** — 長時間コードタスクは** composable 完了ゲート**（オペレータ / モデル / ツールチェーン）で判定。「モデルが終わったと言った」だけでは完了にしません。仕様: [LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · フィクスチャ: [`fixtures/harness/`](fixtures/harness/)。

**2. 複数入口、1 エンジン** — [Tauri 2](https://tauri.app/) デスクトップ **または** 全画面 **`zagens-tui`**（ratatui）**または** ヘッドレス **`zagens`** CLI — いずれも **Kernel V3**（`LiveTurnMachine` + `EffectInterpreter`、イベントソーシング turn、log-first セッション再開）。デスクトップはトレイ・WebView・PTY・sidecar 監督；TUI はターミナル内 3 カラム transcript/composer/inspector + LHT パネル。

**3. 統一コード Agent 面** — デスクトップ Composer は **Auto / Code**（旧 **Office** は Code に移行）。文書は **`load_skill zagens-office`** + 外部 CLI。独立 Office モードは無し。

その他: **CRAFT マルチエージェント**（サブエージェント、fix-loop 判定、P1 ブラックボード — [メモ](docs/craft-v2-improvements.md)）、遅延**シンボル索引**（`.zagens/symbols.json`）、MCP、スキル、Hooks、スケジュールタスク / **night queue**、**`batch_edit`** / **`refactor_imports`** 一括コードツール。

---

## 重点課題

| 課題 | Zagens のアプローチ |
|------|---------------------|
| Agent が途中停止、または早すぎる完了マーク | **段階的完了ゲート** + 長時間タスクパネル（[composable harness](docs/harness/COMPOSABLE_HARNESS.md)） |
| IDE プラグインとターミナル Agent のセッション分断 | 単一 **sidecar** + SQLite スレッド、fork/再開、**リプレイ**、ワークスペーススナップショット |
| 表計算・文書の専用面が欲しい | **`load_skill zagens-office`** + 外部 CLI；デスクトップで成果物を開く |
| ローカルツール実行を盲信しない | 実行ポリシー、ネットワーク規則、パス正規化、承認 UI、runtime トークンは WebView に入れない（[サンドボックス行列](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)） |

---

## 現時点で提供（v0.9.0）

**Office → `zagens-office`（破壊的）:** 内蔵 Office モード / `write_office` / `read_office` と同梱 PBS Python を削除；ドキュメントはスキル **`zagens-office`** + 外部 CLI（`exec_shell` ハードルート）。**デスクトップ・ストリーミング安定性:** マルチターン/復元時の message id 衝突、途中差し替え、lossy delta 重複排除、二重ライブ枠を修正。**Browser P0 + Windows CDP:** 共有 URL ポリシー、セッション allowlist、CDP 操作/スナップショット。**Windows `exec_shell`:** ホスト対応 description、spawn 整合、出力 spill、`[agent] shell`。

**デスクトップ・ストリーミング UX:** 深い tool loop 中も transcript が空白に見えない；transient プロキシエラー後の SSE 再接続；コンパクト Hold パネル + ストリーミング reasoning；Browser プレビュー hint のスロットル。**Harness ファイル変更カード:** フロートスタックのライブ編集リスト（+/- 行数）、Diff へクリック遷移。

**ツール証拠 + 意図コンポジット:** Evidence 封筒（`facts` / `citations` / `uncertainty`）+ citation auditor；`investigate` / `answer_from_repo` / `change_and_verify`；claim↔evidence nudge；`promote_to_context` + 差分 `read_file`；noisy ツールの段階 compact。**共有 model catalog / providers.toml** SSOT；一等 **Moonshot / Kimi K3**。監査 scratchpad 完了ゲート + 強制 import。

**デスクトップ Browser パネル:** 埋め込み WebView（ウィンドウフォールバック可）；Agent ツール `browser_navigate` / `snapshot` / click·type·scroll / `wait` / preview；URL ポリシー + セッション allowlist；YOLO はグローバル自動承認と分離。**Diff 薄層 Git:** workspace status / changes / file-diff / 読み取り専用 PR；Diff タブバッジ；force-push 承認バナー。**night queue** 停止/取消/再試行/クリア。統合ターミナルのライフサイクルと Shell UX。**Zagens Neural Ring** アイコン。

**Harness 2026 H2（Phase 0–4）:** 述語ライブラリ + **`HarnessVerifyLoop`**；**night queue**（`zagens queue` + デスクトップパネル + schedule/hooks）；スキル **stage gate**；**Gate-as-Code**（`zagens gate`）；**`draft_skill`** + promote；T5 **`explore_codebase`** / **`edit_and_check`**；Agent 体检（`GET /v1/agent-health`）；replay pack + **`zagens trace benchmark`**。仕様: [`docs/harness/`](docs/harness/README.md)。

**デスクトップ streaming timeline:** thinking / tool / text を時系列で交互表示、activity bundle、ターン終了時の自動折りたたみ、長ターンの可読性（office / workflow / サブエージェント / browser 折りたたみ）。**サブエージェント step journal**。LHT verify-hygiene + 完了ゲートのライブ状態。

**Kernel V3 エンジン:** イベントソーシング turn — `sessions.db` の `KernelEvent` ログ、`LiveTurnMachine` 計画、`EffectInterpreter` IO、golden リプレイフィクスチャ。仕様: [AGENT_KERNEL_V3.md](docs/tech/AGENT_KERNEL_V3.md)。

**デスクトップ（Tauri）:** Browser + Diff + night-queue 操作；Agent 体检サイドパネル；streaming timeline；**Dusk** テーマ；**git worktree** 並列セッション；**チェックポイント/巻き戻し** と **channels**；モデルプロバイダパネル；セッション overlay；統合 PTY；**Kernel Trace Report** エクスポート。4 言語 UI。

**ターミナル TUI（`zagens-tui`）:** 全画面 3 カラム — セッション rail、ストリーミング transcript、composer（`/model`、`/lht`）、承認モーダル、inspector（files / diff / checklist / **context** / agents / MCP）、折りたたみ LHT 下ペイン、テーマ、セッション復元（`--fresh` で新規）。デスクトップと同一 runtime スレッドと Kernel V3 パス。

**Runtime:** スレッド、MCP、スキル、Hooks、マルチプロバイダ、ビジョン；night-queue / agent-health / symbol-index API；**`GET/PUT/DELETE /v1/threads/{id}/config`**；グローバル **`thread.status`** SSE；**`POST /v1/threads/{id}/events`** チャネル注入。

**ツール（代表）:** ファイル、git、`exec_shell`、T4 `assert_*`、T5 複合、意図コンポジット（`investigate` / `answer_from_repo` / `change_and_verify`）、任意で `web_search` / `fetch_url`、メモリ；Office はスキル **`zagens-office`**。一覧: `crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md)。

---

## 既知の制限（依存前に）

マーケ用チェックリストより、正直なスコープを優先します。

| 項目 | 状態 |
|------|------|
| **デスクトップインストーラ** | **Windows** は [Releases](https://github.com/didclawapp-ai/zagens/releases)。**macOS / Linux デスクトップパッケージ** — 計画中。3 プラットフォーム **`zagens` CLI** と **`zagens-tui`** 提供済み。 |
| **OS サンドボックス強制** | **macOS Seatbelt** — `sandbox-exec` 利用時に強制。**Windows** — ネイティブサンドボックス実装済み（`elevated` 推奨：`zagens sandbox setup` 後に強制；`unelevated` は workspace 書き込み隔離のみ）。設定 → **Sandbox** 初回ウィザード。**Linux** — ポリシー宣言のみ、**OS 未強制**（degraded）。詳細: [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)。 |
| **プロバイダ** | **DeepSeek V4**（Pro / Flash）向けに最適化。API キーはユーザー提供。OpenAI 互換エンドポイントも利用可 — **モデルはホストしません**。 |
| **長時間 & マルチエージェント** | ゲートと CRAFT は**利用可能だが進化中**；エッジケースと新ゲート種別を開発中。 |
| **文書ワークフロー** | 組み込み **Office モード**削除。スキル **`zagens-office`** + 外部 CLI（[備忘](docs/desktop/OFFICE_SCENARIOS.md)）。 |

セキュリティ報告: [`SECURITY.md`](SECURITY.md)。

---

## 今後の方向

公開設計仕様: [`docs/`](docs/README.md)。方向性:

- **プラットフォーム parity** — macOS/Linux デスクトップインストーラ；**Linux** ネイティブサンドボックス（Landlock/bwrap）。Windows ネイティブサンドボックスは 0.7.x で提供済み。
- **信頼できる長時間タスク** — より厳密な完了ゲート、Harness フィクスチャ、リプレイ可能なオペレータワークフロー。
- **Office ワークフロー** — CLI/スキル統合と Pro エンジン。
- **ハードニング** — [CHANGELOG](CHANGELOG.md) と [SECURITY.md](SECURITY.md) で追跡。

---

## クイックスタート

### Zagens デスクトップ（Windows）

[GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) で **Windows** デスクトップインストーラ（`*-setup.exe.zip`）を配布。macOS / Linux デスクトップパッケージは計画中。SmartScreen: [SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md)。

### CLI と TUI — プラットフォーム別

| 入口 | Linux | macOS | Windows |
|------|-------|-------|---------|
| **`zagens-tui`**（全画面ターミナル UI） | ✅ | ✅ | ✅ |
| **`zagens`**（ヘッドレス CLI） | ✅ | ✅ | ✅ |
| **デスクトップアプリ** | —（TUI を使用） | —（TUI を使用） | ✅ インストーラ |

**プリビルド**（[Releases `zagens-v0.9.0`](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.9.0)）、**`cargo install`**（crates.io）、**ソースビルド**（下記）のいずれかで導入。

**Rust 前提**（`cargo install` / ソースのみ）: [rustup](https://rustup.rs/)（Rust **1.88+**；CI は 1.96）。Linux/macOS は `source "$HOME/.cargo/env"`、Windows はターミナル再起動。

#### Linux（Ubuntu / Debian）

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config libssl-dev libdbus-1-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# TUI（初回コンパイルは 10–30 分程度）
cargo install zagens-cli --version 0.9.0 --bin zagens-tui --features tui --locked

# ヘッドレス CLI（任意）
cargo install zagens-cli --version 0.9.0 --bin zagens --locked
```

**プリビルド**（Rust 不要）: [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.9.0) から `zagens-tui-x86_64-unknown-linux-gnu` および/または `zagens-x86_64-unknown-linux-gnu` を取得し、`.sha256` を検証、`chmod +x` して `PATH` に配置。

```bash
zagens-tui              # 前回セッション復元
zagens-tui --fresh      # 新規セッション
```

#### macOS

```bash
xcode-select --install    # C ツールチェーンがない場合
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

cargo install zagens-cli --version 0.9.0 --bin zagens-tui --features tui --locked
cargo install zagens-cli --version 0.9.0 --bin zagens --locked   # 任意
```

**プリビルド:** [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.9.0) の `zagens-tui-x86_64-apple-darwin`（Intel）または `zagens-tui-aarch64-apple-darwin`（Apple Silicon）。

#### Windows

**プリビルド（最速）:** [Releases](https://github.com/didclawapp-ai/zagens/releases/tag/zagens-v0.9.0) — `zagens-tui-x86_64-pc-windows-msvc.exe`、`zagens-x86_64-pc-windows-msvc.exe`（+ `.sha256`）。フォルダを `PATH` に追加するか、`.exe` を `PATH` 上のディレクトリへコピー。

**crates.io**（先に [Rust for Windows](https://rustup.rs/) をインストール）:

```powershell
cargo install zagens-cli --version 0.9.0 --bin zagens-tui --features tui --locked
cargo install zagens-cli --version 0.9.0 --bin zagens --locked
```

### crates.io（全プラットフォーム）

```bash
cargo install zagens-cli --version 0.9.0 --bin zagens-tui --features tui --locked   # TUI
cargo install zagens-cli --version 0.9.0 --bin zagens --locked                   # CLI
cargo install zagens-cli --version 0.9.0 --bin zagens-runtime --locked           # HTTP sidecar（任意）
```

### ソースから — デスクトップ

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # zagens-runtime を crates/desktop/binaries/ にコピー

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API キー: Zagens 設定、または ~/.zagens/config.toml
```

### ソースから — ターミナル TUI

```bash
cargo build -p zagens-cli --features tui --bin zagens-tui
./target/debug/zagens-tui          # 前回セッション復元；--fresh で新規
```

**API キー:** `DEEPSEEK_API_KEY`、`~/.zagens/config.toml`、または TUI の `/api-key` / 初回オンボーディング。

**CLI の例:**

```bash
zagens doctor
zagens exec 'summarize src/' --json
zagens exec 'refactor auth module' --auto
zagens serve --http --port 7878
```

設定: [config.example.toml](config.example.toml)。

---

## アーキテクチャ

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Zagens Desktop  │  │   zagens-tui     │  │  zagens CLI      │
│  Tauri + WebView │  │  ratatui TUI     │  │  exec / serve    │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │ HTTP+SSE (loopback) │ in-process          │ in-process / HTTP
         ▼                     ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  zagens-runtime sidecar  ·  Kernel V3 turn エンジン             │
│  LiveTurnMachine → EffectInterpreter → V3TurnHost               │
│  /v1/threads · MCP · skills · tools · kernel_events log         │
└───────────────────────────────┬─────────────────────────────────┘
                                ▼
         zagens-core · runtime-orchestrator · runtime-adapters
```

境界: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · Kernel V3: [`docs/tech/AGENT_KERNEL_V3.md`](docs/tech/AGENT_KERNEL_V3.md) · HTTP: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md)。

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
├── crates/desktop/        # Tauri デスクトップ
├── crates/runtime-server/ # zagens-runtime sidecar · zagens CLI · zagens-tui（feature `tui`）
├── crates/core/           # Kernel V3 エンジン
├── docs/                  # 公開設計仕様
├── fixtures/harness/      # LHT / kernel リプレイ
└── config.example.toml
```

---

## ライセンス

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors。追加帰属: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE)。
