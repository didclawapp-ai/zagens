# DS Pick ↔ deepseek-tui sidecar 连接稳定性治理方案

> 状态：已实施 v1（详见第 13 节「实施回顾」）
> 作者：（待补）
> 创建日期：2026-05-15
> 实施完成日期：2026-05-15
> 关联问题：DS Pick 启动后 WebView 周期性出现
> `127.0.0.1:7878/health  net::ERR_CONNECTION_REFUSED`，
> `~/.deepseek/logs/supervisor.log` 中频繁出现
> `sidecar unresponsive (health timeout, 6 failures, ...); restarting`。
> 关联代码：[crates/desktop/src/sidecar.rs](../../crates/desktop/src/sidecar.rs)、
> [crates/tui/src/runtime_api.rs](../../crates/tui/src/runtime_api.rs)、
> [crates/tui/src/runtime_threads.rs](../../crates/tui/src/runtime_threads.rs)、
> [crates/desktop/web-ui/src/api/client.ts](../../crates/desktop/web-ui/src/api/client.ts)。

---

## 0. 摘要（TL;DR）

WebView 看到的 `ERR_CONNECTION_REFUSED` **不是网络/CORS/Token 问题**，而是 supervisor 在那一刻确实把 sidecar 杀掉了：

1. sidecar 是有状态、IO 密集的进程；恢复一个 300+ 消息的会话需要 ~750 次原子写文件（约 1.5–3 s）；
2. supervisor 用与业务**同一条 HTTP 路径**做探活（`GET /v1/sessions` 走 `SessionManager`），并和业务共享 `tokio::task::spawn_blocking` 池；
3. 当业务繁忙（resume / persist-session / list_sessions）时，探针被排在 blocking 池后面 → 超时 → `failures += 1`；
4. 连续 6 次失败（≈30 s）后 supervisor `kill -9` → 端口空窗 → 前端 `ERR_CONNECTION_REFUSED` → 新 sidecar 起来又被打多个并发 resume → 再次堵塞。

这是**架构层缺陷**，单点调参解决不了。本方案分四个阶段把它治本：

- **阶段 1（Quick Wins，代码层）**：拆分探针端点 + 区分错误类型 + 缓存 SessionManager。
- **阶段 2（Resume 路径异步化）**：把 seed 从请求路径移到后台 task，前端订阅进度。
- **阶段 3（Sidecar 显式 READY 信号）**：消除"bind 成功 ≠ 可用"的隐式假设。
- **阶段 4（带外探针通道）**：探活走 stdin/stdout 行协议或命名管道，与业务路径物理隔离。

阶段 1 + 2 + 3 完成后，目测可永久消除当前抖动；阶段 4 是把"很少出问题"提升为"结构上不可能出问题"。

---

## 1. 现状与证据

### 1.1 故障观测

WebView (DevTools)：

```
127.0.0.1:7878/health                                     Failed to load resource: net::ERR_CONNECTION_REFUSED
http://127.0.0.1:7878/v1/threads/thr_abb12e06/snapshots?limit=50   net::ERR_CONNECTION_REFUSED
```

`~/.deepseek/logs/supervisor.log`：

```
2026-05-15T17:21:16.278 supervisor start (port=7878, sidecar=deepseek-tui)
2026-05-15T17:21:53.990 health restored after 1 failure(s)
... (大量 health restored after N failure(s))
2026-05-15T20:35:24.519 sidecar unresponsive (health timeout, 6 failures, port=7878, token_len=36); restarting.
2026-05-15T20:48:09.829 sidecar unresponsive (health timeout, 6 failures, ...); restarting.
2026-05-15T21:34:53.226 sidecar unresponsive (health timeout, 6 failures, ...); restarting.
```

`~/.deepseek/logs/sidecar.log`（每次 supervisor 重启后必然出现）：

```
[deepseek-runtime] bound 127.0.0.1:7878, serving (+213.569ms)
Runtime API listening on http://127.0.0.1:7878
[resume-session] load_session done in 0.0s, 350 messages
[resume-session] seed done in 1.8s total, thread=thr_abb12e06
```

DevTools 报错里的 `thr_abb12e06` 正是 supervisor 21:31 那次重启刚 seed 出来的 thread —— 证明 WebView 拿到的就是这条短命 thread。

### 1.2 关键代码事实

| 位置 | 行为 |
|------|------|
| [`sidecar.rs` 探针](../../crates/desktop/src/sidecar.rs) | `sidecar_ready` 同时调用 `/health` + `/v1/sessions`（Bearer Token），后者走 SessionManager。 |
| [`sidecar.rs` 阈值](../../crates/desktop/src/sidecar.rs) | `HEALTH_CHECK_INTERVAL_SECS = 5`，`MAX_HEALTH_FAILURES = 6`，`SIDECAR_PROBE_HTTP.timeout = 15s`，`connect_timeout = 400ms`。 |
| [`sidecar.rs` 杀进程](../../crates/desktop/src/sidecar.rs) | 失败累计后 `child.kill()`，并用 PowerShell `Stop-Process` 兜底回收占用 7878 的进程。 |
| [`runtime_api.rs` resume 路由](../../crates/tui/src/runtime_api.rs) | `/v1/sessions/{id}/resume-thread` 同步执行 load + seed，350 条消息约 1.8s。 |
| [`runtime_threads.rs::seed_thread_from_messages`](../../crates/tui/src/runtime_threads.rs) | 对每个 (user, assistant) 对调用 2 次 `save_item` + 1 次 `save_turn`，均为 `write_atomic`（tmp + fsync + rename）。 |
| [`runtime_api.rs` 路由表](../../crates/tui/src/runtime_api.rs) | `/health` 在公开层，其余 `/v1/*` 全部走 `require_runtime_token` middleware。 |
| [`client.ts` 重试](../../crates/desktop/web-ui/src/api/client.ts) | WebView 对网络错误做 5 次指数退避（350ms 起步），但 supervisor 端没有对应的"宽容窗口"。 |

### 1.3 架构图（现状）

```
┌─────────────────────────────────────────────────────────────────────┐
│  DS Pick (Tauri 主进程)                                              │
│                                                                     │
│  ┌──────────────┐                  ┌────────────────────────────┐   │
│  │ WebView (UI) │ ──HTTP /v1/*────►│                            │   │
│  └──────────────┘                  │                            │   │
│                                    │   axum (单一 HTTP 端面)    │   │
│  ┌──────────────┐ ──HTTP /health──►│   ↓                        │   │
│  │  supervisor  │ ──HTTP /v1/sess─►│   tokio blocking pool       │  │
│  │   task       │ ◄── kill -9 ────│   ↓                        │   │
│  └──────────────┘                  │   SessionManager (磁盘 IO) │   │
│                                    └────────────────────────────┘   │
│                                              ▲                      │
│                                              │ stdout/stderr only   │
│                                    ┌─────────┴──────────┐           │
│                                    │  deepseek-tui      │           │
│                                    │  serve (子进程)    │           │
│                                    └────────────────────┘           │
└─────────────────────────────────────────────────────────────────────┘
```

**问题：**

1. supervisor 和 WebView 通过同一端面（HTTP），探活与业务共享线程池。
2. supervisor → 子进程的下行通道**只有** `kill`（没有"draining / are-you-busy" 信号）。
3. 子进程 → supervisor 的上行通道**只有** HTTP 响应和 stdout 日志，没有结构化的 READY/状态报告。

---

## 2. 根因分级（必须分清）

### 2.1 架构层（不解决就永远存在）

| ID  | 描述 | 当前症状 |
|-----|------|---------|
| A1  | supervisor 用网络探针做生命周期管理，但 sidecar 是同机进程 | "假死"和"真死"无法区分 |
| A2  | 探针走业务路径（`/v1/sessions` → SessionManager），与 resume 抢同一 blocking 池 | 业务忙 = 探针超时 = 误判 |
| A3  | sidecar 没有显式 "READY" 信号；`bind` 成功立刻接收并发请求 | 启动后被打 N 个 resume 立刻饱和 |
| A4  | `/v1/sessions/{id}/resume-thread` 是同步路径，seed 在请求里完成 | 单次 resume 阻塞 1.5–3 s |
| A5  | 持久化模型 = 每条 turn/item 一次 `write_atomic`（tmp+fsync+rename） | 350 条消息 = 750+ 次 fsync ≈ 几秒 |

### 2.2 代码层（值得修，但单独修不够）

| ID  | 描述 |
|-----|------|
| B1  | `runtime_api_accepts_token` 走 `/v1/sessions`，应改为零成本鉴权探针。 |
| B2  | `MAX_HEALTH_FAILURES = 6` 不区分"connect refused"（端口空）与"read timeout"（端口在但忙）。 |
| B3  | supervisor 探针超时 15s + 5s 间隔 + 6 次累计 = 实际容忍 < 30s 业务高峰。 |
| B4  | `SessionManager::new` 在每个 handler 内部重建，重复扫描 sessions 目录。 |
| B5  | 前端在 thread 切换时可能并发触发多个 resume；无窗口可见性节流。 |
| B6  | supervisor 兜底用 PowerShell `Stop-Process` 杀监听端口的进程；杀错的代价比放过高。 |

> 一句话：**B 类问题让 A 类问题更容易触发，但 A 类问题才是抖动的来源。**

---

## 3. 目标与非目标

### 3.1 目标

1. WebView 在正常工作流（启动、切 thread、保存设置）下**绝不**看到 `ERR_CONNECTION_REFUSED`。
2. supervisor 误杀率 → 0；保留**真死**的快速回收能力（< 1s 内重启）。
3. 大会话（500+ 消息）的 resume 不阻塞任何探针或其他请求。
4. 不引入新的进程/依赖；保持 Tauri sidecar 模型。
5. 跨平台一致（Windows / macOS / Linux）。

### 3.2 非目标

- 不在本方案内做 SQLite 迁移（属于阶段 5，单独立项）。
- 不重写 Tauri 进程托管层（继续走 `tokio::process::Command`）。
- 不修改 deepseek API 协议（OpenAI compatible 部分保持不变）。

---

## 4. 设计原则

1. **探活与业务物理隔离**：探活不得触碰任何业务路径下使用的同一线程池、锁、磁盘队列。
2. **状态显式化**：sidecar 的生命周期必须有 `starting → ready → serving → draining → exited` 五态，且每次状态转换都向 supervisor 显式通知。
3. **请求路径只承载"快"的事情**：任何 > 200ms 的操作（resume、persist、compact、large list）必须异步化；HTTP handler 立即返回 task_id。
4. **错误语义分级**：网络错误必须区分 *connection refused / connection reset / read timeout / 5xx* —— 它们对应不同的 supervisor 决策。
5. **代码可观测**：所有 supervisor 决策（探针成功/失败、kill、restart、port-free 等待）继续写入 `supervisor.log`，加入结构化字段（reason、failure_kind、elapsed_ms）便于事后归因。

---

## 5. 方案分阶段详述

### 阶段 1｜Quick Wins（代码层，1–2 个工作日）

#### 5.1.1 新增内部探针端点（不走业务路径）

**位置：** [`crates/tui/src/runtime_api.rs`](../../crates/tui/src/runtime_api.rs)

新增公开层路由（与 `/health` 同级，**不**经 `require_runtime_token`）：

```rust
.route("/internal/probe", get(internal_probe))
```

实现：

```rust
#[derive(serde::Serialize)]
struct InternalProbeResponse {
    status: &'static str,        // 始终 "ok"
    pid: u32,
    started_at_ms: u128,
    token_fingerprint: String,    // sha256(token)[..16]，用于 supervisor 判定 token 一致
    version: &'static str,
}

async fn internal_probe(State(state): State<RuntimeApiState>) -> Json<InternalProbeResponse> {
    Json(InternalProbeResponse {
        status: "ok",
        pid: std::process::id(),
        started_at_ms: state.process_started_at_ms,
        token_fingerprint: state.token_fingerprint.clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}
```

> **零业务依赖**：不访问 SessionManager / RuntimeThreadStore / blocking 池；
> 直接从 `RuntimeApiState` 拿预先计算好的常量字段返回。

**`RuntimeApiState` 新增字段：**

- `process_started_at_ms: u128`（启动时计算一次）
- `token_fingerprint: Arc<String>`（启动时 `sha256(token)[..16]`）

`token_fingerprint` 用于验证 supervisor 与 sidecar 的 token 同源 —— 替代当前用 `/v1/sessions` 返回 401/200 来判断 token 是否匹配的 hack（解决 [`sidecar.rs:311-316`](../../crates/desktop/src/sidecar.rs) 那段"`/health` ok 但 `/v1/*` 401 → 杀监听者"的纠缠逻辑）。

#### 5.1.2 supervisor 探针改造

**位置：** [`crates/desktop/src/sidecar.rs`](../../crates/desktop/src/sidecar.rs)

```rust
enum ProbeOutcome {
    Ok,
    Refused,             // TCP connect 失败（端口确实空）
    Timeout,             // 连上了但读响应超时（sidecar 忙）
    TokenMismatch,       // token_fingerprint 不一致
    Other(String),
}

async fn probe_sidecar(port: u16, expected_fp: &str) -> ProbeOutcome {
    use reqwest::Error as RE;
    let url = format!("http://127.0.0.1:{port}/internal/probe");
    match SIDECAR_PROBE_HTTP.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<InternalProbeResponse>().await {
                Ok(body) if body.token_fingerprint == expected_fp => ProbeOutcome::Ok,
                Ok(_) => ProbeOutcome::TokenMismatch,
                Err(e) => ProbeOutcome::Other(e.to_string()),
            }
        }
        Ok(resp) => ProbeOutcome::Other(format!("HTTP {}", resp.status())),
        Err(e) if e.is_connect() => ProbeOutcome::Refused,
        Err(e) if e.is_timeout() => ProbeOutcome::Timeout,
        Err(e) => ProbeOutcome::Other(e.to_string()),
    }
}
```

**新的失败计数策略（关键）：**

```rust
struct ProbeCounters {
    connect_refused: u32,   // 触发 kill 的阈值低（≤ 2）
    busy_timeouts:   u32,   // 阈值高（≤ 12，≈60s）
    token_mismatch:  u32,   // ≥ 1 即处理（端口被陌生进程占用，需 reclaim）
}

const MAX_CONNECT_REFUSED: u32 = 2;
const MAX_BUSY_TIMEOUTS:   u32 = 12;
```

判定逻辑：

- `Refused` × 2（10s 内）→ 子进程真挂了 / 端口空，立即 kill 残骸 + 重启。
- `Timeout` × 12（60s 内）→ 真正"hang"，重启；中间任何一次 Ok 重置计数。
- `TokenMismatch` 一次 → 走 `wait_loopback_listen_port_free` 杀监听者，然后重启。
- 仍保留 `child.try_wait()` 快路径：进程已退出 → 立即重启。

#### 5.1.3 缓存 SessionManager / RuntimeThreadStore

**位置：** `RuntimeApiState`、`get_session` / `resume_session_thread` / `list_sessions`。

把 `SessionManager` 改为 `Arc<SessionManager>`，启动时构造一次；handler 内部不再 `SessionManager::new(...)`。

#### 5.1.4 supervisor 日志结构化

`supervisor.log` 改为单行 JSON 或带 `key=value` 字段：

```
2026-05-15T20:35:24.519 event=restart reason=busy_timeout failures=12 elapsed_ms=60123 port=7878
```

便于事后用 `rg event=restart` 直接统计抖动频率。

#### 5.1.5 验收

- `cargo build --workspace`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace --all-features` 全绿。
- 手动测试：人为在 sidecar 内部 `tokio::time::sleep(Duration::from_secs(20))` 阻塞 `/v1/sessions`，supervisor **不**应触发 restart（`/internal/probe` 不受影响）。
- 关闭 sidecar 进程 → supervisor 在 ≤ 5s 内重启（`Refused` 路径仍快）。

---

### 阶段 2｜Resume / 长任务异步化（3–5 个工作日）

#### 5.2.1 新协议

把单一同步接口拆为：

```
POST /v1/sessions/{id}/resume-thread          → 同步立即返回
  { task_id: "task_xxx", thread_id: "thr_yyy", state: "seeding" }

GET  /v1/tasks/{task_id}                       → 轮询或 SSE
  { state: "seeding" | "ready" | "error",
    progress: { items_written, items_total },
    error?: "..." }

GET  /v1/tasks/{task_id}/events  (SSE)        → 实时进度
```

`thread_id` 在返回时已经在 `RuntimeThreadStore` 注册（轻量元数据），但 turns/items 还在写入中。

#### 5.2.2 seed 后台化

**位置：** [`crates/tui/src/runtime_api.rs::resume_session_thread`](../../crates/tui/src/runtime_api.rs)。

```rust
async fn resume_session_thread(...) -> Result<(StatusCode, Json<ResumeSessionResponse>), ApiError> {
    let thread = state.runtime_threads.create_thread(...).await?;
    let task = state.task_manager
        .spawn_resume_task(thread.id.clone(), session_id.clone())
        .await?;
    Ok((StatusCode::ACCEPTED, Json(ResumeSessionResponse {
        task_id: task.id,
        thread_id: thread.id,
        state: "seeding",
    })))
}
```

#### 5.2.3 seed 队列与并发限制

引入 `tokio::sync::Semaphore::new(1)` 作为全局 **seed gate**，保证：

- 即使前端同时打开 N 个 thread，seed 一次只跑一个，blocking 池永远不会被 N×750 次 fsync 打爆；
- supervisor `/internal/probe` 始终无阻；
- WebView 通过 SSE 看到当前 seed 排在第几位。

#### 5.2.4 seed 内部批写（首轮优化，不上 SQLite）

[`seed_thread_from_messages`](../../crates/tui/src/runtime_threads.rs) 改进：

- 把 N 次 `save_item` / `save_turn` 合并成"先在内存里构造好全部 `TurnRecord` / `TurnItemRecord`，最后一次写一个 `thread_seed.jsonl` 索引文件 + 仍按需要写各自的小文件" → 但用 `tokio::task::spawn_blocking` 包**整个批**，避免在 tokio 调度间隙穿插探针请求。
- 每写 50 条让出一次（`tokio::task::yield_now().await` 或 `tokio::time::sleep(Duration::from_millis(0)).await`）—— 给 axum 其他请求机会。
- 可选：把 `write_atomic` 改为先 `write_all` 到 tmp、最后批量 rename（rename 是廉价的）。

> 真正的"每条消息 1 次 fsync → 750 次 fsync"问题留到阶段 5（SQLite）解决；这里只是缓解。

#### 5.2.5 前端联动

[`crates/desktop/web-ui/src/api/client.ts`](../../crates/desktop/web-ui/src/api/client.ts) 与 thread 视图：

- `resumeSession()` 拿到 `{ thread_id, task_id }` 后立即跳转到 thread 视图（**不**阻塞 UI）。
- thread 视图根据 `state` 渲染骨架屏 / "正在加载历史消息 (123/350)…"。
- `task.state` 转 `ready` 时再渲染完整 turns。
- thread 视图自身的 `/v1/threads/{id}/snapshots` 在 `state !== "ready"` 时跳过请求，避免在 seed 期间打不必要的查询。

#### 5.2.6 验收

- 恢复 1000 条消息的会话，WebView 切换流畅，`/internal/probe` 持续 200 ms 内响应。
- supervisor.log 中**不再**出现 `health restored after N failure(s)`（除非真有进程异常）。
- 并发恢复 3 个会话，吞吐序列化但都最终 `state=ready`。

---

### 阶段 3｜Sidecar 显式 READY 信号（1–2 天）

#### 5.3.1 STDOUT 协议

sidecar 在 axum `serve` 真正进入 accept 循环、并完成所有启动期 warmup（含 token_fingerprint 计算、SessionManager 构造）后，**第一行**向 stdout 打印：

```
DS_PICK_READY {"port":7878,"pid":12345,"token_fp":"ab12cd34...","version":"0.8.6"}
```

并 flush。这一行是 supervisor 唯一信任的"我现在可以接客"的信号。

#### 5.3.2 supervisor 读取方式

`spawn_sidecar` 改为：

- 不把 stdout 直接重定向到 `sidecar.log` 文件，改成 piped；
- 起一个 task 读 stdout 行，解析 `DS_PICK_READY {...}` 后通过 `tokio::sync::oneshot` 通知主循环；
- 其余行原样 append 到 `sidecar.log`（保持现状）；
- 收到 `READY` 之前**不**做 HTTP 探针、**不**接受任何重启请求；
- `READY` 超时（比如 60s）→ kill + 走 startup 失败路径。

#### 5.3.3 兼容性

- 旧版本 sidecar 没有 `DS_PICK_READY` → supervisor fallback 到 HTTP probe（保留现有路径作为兜底）。
- 通过 `version` 字段决定后续是否启用阶段 2 的异步 resume 协议。

#### 5.3.4 验收

- 启动延迟从 spawn → ready 的指标进入 `supervisor.log`：`event=ready_signal elapsed_ms=XYZ`。
- 启动期不再有任何 HTTP 探针失败。

---

### 阶段 4｜带外探针通道（结构性闭合，3–5 天）

> 这是真正让"探活与业务物理隔离"落地的步骤。完成阶段 1–3 后业务上已经很稳，阶段 4 是把"很稳"提升为"结构上不可能再被业务影响"。

#### 5.4.1 通道选择

| 方案 | 优点 | 缺点 |
|------|------|------|
| **stdin/stdout 行协议** | 跨平台、零外部依赖、Tauri 已有重定向 | 与日志共用 stdout，需要协议帧化 |
| 命名管道（Win）/ Unix socket | 物理隔离最干净 | 平台差异大，要写两份 |
| 第二个 loopback 端口 | 实现简单 | 还是 HTTP，没真正解耦 |

**推荐：stdin/stdout 行协议**，与阶段 3 的 READY 行共用同一信道。

#### 5.4.2 协议（最小集）

行协议（每行一个 JSON），supervisor 写到 sidecar stdin，sidecar 回写到 stdout（与 READY 同一流，由前缀区分）：

```
→  {"op":"ping","seq":42}
←  {"op":"pong","seq":42,"pid":12345,"uptime_ms":1234567}

→  {"op":"drain"}                             // 优雅退出请求
←  {"op":"drain","state":"draining"}
←  {"op":"drain","state":"done"}              // 然后进程退出
```

sidecar 用一个**独立的 tokio task** 处理 stdin：

```rust
tokio::spawn(async move {
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // 解析并回写 stdout（同样独立 task，不经 axum、不经 blocking 池）
    }
});
```

#### 5.4.3 supervisor 决策表

| 信号 | supervisor 行为 |
|------|-----------------|
| pong 间隔 < 1 s | Ok |
| pong 缺失 1 次 | warn |
| pong 缺失 3 次（≈3 s） | 发 drain；同时启用 HTTP 探针交叉验证 |
| pong 缺失 6 次 且 HTTP 也 timeout | 判 hang，kill + restart |
| stdin pipe EOF | 子进程已退出，立即重启 |

注意：**HTTP 探针不再是判死的主依据**，仅作为"sidecar 是否还能服务 HTTP"的次级信号；判活的主依据回到带外通道。

#### 5.4.4 验收

- 在 sidecar 内部模拟 axum 死锁（不响应 HTTP，但 stdin task 仍工作）→ supervisor 不应 kill。
- 在 sidecar 内部 panic → stdin pipe EOF → supervisor 在 < 1s 内重启。
- 关掉 sidecar 二进制（外部 `taskkill /F`）→ 同样 < 1s 重启。

---

## 6. 总体改动清单

| # | 文件 | 改动 |
|---|------|------|
| 1 | [`crates/tui/src/runtime_api.rs`](../../crates/tui/src/runtime_api.rs) | 新增 `/internal/probe`；`RuntimeApiState` 新增 `process_started_at_ms` / `token_fingerprint`；缓存 `SessionManager`。 |
| 2 | [`crates/tui/src/runtime_api.rs`](../../crates/tui/src/runtime_api.rs) | `resume_session_thread` 改为返回 `task_id`，seed 后台化。 |
| 3 | [`crates/tui/src/runtime_threads.rs`](../../crates/tui/src/runtime_threads.rs) | `seed_thread_from_messages` 批写 + yield；引入 seed gate。 |
| 4 | `crates/tui/src/task_manager.rs`（如已存在） | 新增 `spawn_resume_task`；任务状态机；SSE 接口。 |
| 5 | `crates/tui/src/main.rs` | 启动期完成 warmup 后向 stdout 打印 `DS_PICK_READY {...}`。 |
| 6 | [`crates/desktop/src/sidecar.rs`](../../crates/desktop/src/sidecar.rs) | 探针指向 `/internal/probe`；分类计数；READY 行监听；带外行协议。 |
| 7 | [`crates/desktop/web-ui/src/api/client.ts`](../../crates/desktop/web-ui/src/api/client.ts) | `resumeSession()` 返回 task_id；新增 task 进度 API。 |
| 8 | `crates/desktop/web-ui/src/pages/Thread*.tsx` | 骨架屏 + 进度提示；seeding 状态下抑制衍生请求。 |
| 9 | `docs/desktop/` | 本方案 + 实施后更新 [PREVIEW_ARCHITECTURE.md](./PREVIEW_ARCHITECTURE.md) 中的进程图。 |

---

## 7. 风险与回滚

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| `/internal/probe` 暴露内部信息（pid/version） | 低 | 仅 loopback；CORS 仍受限制 | token_fingerprint 不是原 token；其余信息不敏感 |
| 异步 resume 改变前端时序，回归 thread 视图 bug | 中 | 中 | feature flag `DS_PICK_ASYNC_RESUME=0` 可回退到同步 |
| READY 行协议遇到旧 sidecar | 低 | 低 | 60s timeout 后 fallback 到 HTTP 探针 |
| stdin pipe 与日志共用 stdout 引入解析错误 | 中 | 低 | 行协议加固定前缀 `DS_PICK\t`；非前缀行原样写 sidecar.log |
| 阶段 2 改动较多，影响 task 表 schema | 中 | 中 | 与 `runtime_schema_version` 联动；提供 migrate |

回滚策略：所有阶段均不修改持久化数据格式（阶段 5 才会），任何阶段都可独立 revert。

---

## 8. 实施时间表（建议）

| 周次 | 目标 |
|------|------|
| W1   | 阶段 1 全量上线；观察一周 supervisor.log，确认 `event=restart` 仅在真死时发生。 |
| W2–W3 | 阶段 2：异步 resume + SSE 进度；前端骨架屏；回归大会话场景。 |
| W4   | 阶段 3：READY 行；联调启动期。 |
| W5   | 阶段 4：带外行协议；模拟 hang/panic 注入测试；最终切换主探针为 stdin/stdout。 |

---

## 9. 验收指标

部署后连续运行 72 小时，需要满足：

1. `supervisor.log` 中 `event=restart` 出现次数 ≤ 真正进程异常次数（理想 0 次）。
2. WebView DevTools 不再出现 `ERR_CONNECTION_REFUSED`（窗口启动 + 切换 thread + 保存设置 + 大会话恢复 全场景）。
3. 1000 条消息的会话 resume：HTTP 立即返回 ACCEPTED（< 100ms），seed 在后台完成（< 5s），期间 `/internal/probe` 持续 < 50ms。
4. kill -9 sidecar → supervisor < 1s 内检测并重启（带外通道 + READY 行）。
5. `cargo clippy --workspace --all-targets --all-features` 与 `cargo test --workspace --all-features` 全绿。

---

## 10. 后续（阶段 5 预告，不在本方案范围）

- 把 `SessionManager` / `RuntimeThreadStore` 的 JSON-per-file 持久化层迁到 SQLite（WAL 模式），每条 turn/item 写从一次 fsync ≈ 5–10 ms 降到 ~50 µs。
- 350 条消息的 resume 从"读取 + 750 次 fsync"变成"单事务 + 一次 fsync"（< 50 ms）。
- 顺便获得：会话全文检索、按时间区间清理、并发安全的导入/导出。
- 单独立项立计划，预计 1–2 周。

---

## 11. 附录 A：相关现有约束（来自 AGENTS.md）

- 必须保持 stable Rust，不引入 nightly feature。
- 任何任务结束前需跑 `cargo fmt`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace --all-features`。
- Windows 上 sidecar 启动需 `CREATE_NO_WINDOW`，不要弹黑框。
- token 仅通过 `DEEPSEEK_RUNTIME_TOKEN` 环境变量传递，不进 `ps`。
- 不在未经维护者同意的情况下接入第三方 SDK / SaaS。

## 12. 附录 B：当前阈值参考（修订前 / 后）

| 项 | 修订前 | 修订后 |
|----|--------|--------|
| `HEALTH_CHECK_INTERVAL_SECS` | 5 | 5（不变） |
| `MAX_HEALTH_FAILURES`（合并计数） | 6 | 拆分为 `MAX_CONNECT_REFUSED=2` / `MAX_BUSY_TIMEOUTS=12` |
| `SIDECAR_PROBE_HTTP.timeout` | 15s | 3s（探针路径已无业务依赖，无需 15s） |
| `STARTUP_FIRST_DELAY_MS` | 60 | 由 READY 行替代，仅作 fallback |
| `MAX_STARTUP_RETRIES` | 60 | 由 READY 行替代，仅作 fallback |
| `RESTART_DEBOUNCE_MS` | 450 | 450（不变） |
| seed 并发 | 无限制 | semaphore=1 |
| resume HTTP 行为 | 同步 1.5–3 s | 异步立即 202 |

---

## 13. 实施回顾

> 本节在方案落地后追加，用于记录"实际做了什么"与"哪些地方与计划有出入"。

### 13.1 状态总览

| 阶段 | 计划 | 实施 |
|------|------|------|
| 1 — Quick Wins | `/internal/probe` + 错误分类计数 + `SessionManager` 缓存 | ✅ 已实施 |
| 2 — Resume 异步化 | `task_id` + seed gate + `/v1/tasks/{id}` 进度查询 | ✅ 已实施 |
| 3 — `DS_PICK_READY` 信号 | sidecar 启动后向 stdout 输出 READY 行，supervisor 解析后进入探活 | ✅ 已实施 |
| 4 — 带外 stdin/stdout 行协议 | sidecar 独立 stdin task 处理 PING/DRAIN，supervisor ping 循环 + 1s 等 pong | ✅ 已实施 |

### 13.2 关键落点

- 探针端点：[crates/tui/src/runtime_api.rs](../../crates/tui/src/runtime_api.rs)
  - 新增公开层 `internal_probe` handler，与 `/health` 同层，不经 `require_runtime_token`。
  - `RuntimeApiState` 新增 `process_started_at_ms` / `token_fingerprint`。
- supervisor 探针 & 行协议：[crates/desktop/src/sidecar.rs](../../crates/desktop/src/sidecar.rs)
  - 探针目标切到 `/internal/probe`；校验 `token_fingerprint` 替代旧的 `/v1/sessions` 401/200 路径。
  - 错误分类 `ProbeOutcome { Ok / Refused / Timeout / TokenMismatch / Other }`，与 `MAX_CONNECT_REFUSED` / `MAX_BUSY_TIMEOUTS` 对应。
  - READY 行解析 + stdin ping 循环（≈1s 等 pong）。
- Resume 异步化：[crates/tui/src/runtime_api.rs](../../crates/tui/src/runtime_api.rs) + [crates/tui/src/runtime_threads.rs](../../crates/tui/src/runtime_threads.rs)
  - `resume_session_thread` 返回 `202 Accepted` + `{ task_id, thread_id, state: "seeding" }`。
  - `ResumeTaskTracker` + `Semaphore` 实现 seed gate；`/v1/tasks/{id}` 暴露进度。
  - `seed_thread_from_messages` 在 `spawn_blocking` 中批量执行，期间 yield。
- 前端联动：[crates/desktop/web-ui/src/api/client.ts](../../crates/desktop/web-ui/src/api/client.ts) 及对应 thread 视图
  - `resumeSession()` 拿 `task_id` 后立即跳转 thread 视图，按 `state` 渲染骨架屏 / 完整 turns。

### 13.3 验证结果

| 检查 | 命令 | 结果 |
|------|------|------|
| 工作区编译 | `cargo check --workspace --all-features` | ✅ exit 0；仅 3 个 `dead_code` 警告（位于 `crates/tui/src/symbol_index.rs`，pre-existing，与本次改动无关） |
| 工作区 lint | `cargo clippy --workspace --all-targets --all-features` | ✅ exit 0；56 个 warning 全部位于 `tools/office_write.rs` / `tools/search.rs` / `tools/subagent/*` / `symbol_index.rs`（pre-existing，与本次改动无关） |
| 严格 lint | `cargo clippy ... -- -D warnings` | ❌ 因上述 pre-existing warning 升级为 error；**与本次治理无关**，建议另立清理 PR 处理 |
| 桌面 crate 资源构建 | `cargo clippy -p deepseek-desktop` | ⚠️ Trae 沙箱里 `tauri-build` 调用 Windows `rc.exe` 触发 std panic（`Result::unwrap` on `"操作成功完成"`），需在本地终端验证；非业务逻辑问题 |
| 全量测试 | `cargo test --workspace --all-features` | ⏳ 尚未在本地终端跑完，建议合并前补做 |

### 13.4 与原计划的差异

- **路由命名**：原 spec 提到 `/v1/sessions/{id}/resume`，实际仓库历史命名为 `/v1/sessions/{id}/resume-thread`，本次实施保持原有路径，不做不必要的 break。
- **行协议前缀**：实际落地保留以 `DS_PICK_` 前缀的行作为协议帧（READY / PING / PONG / DRAIN 等），非前缀行原样落 `sidecar.log`，与方案 5.4.4 一致。
- **测试与 lint**：受 Trae 沙箱限制，`cargo test --workspace` 与 `cargo fmt --all -- --check` 未在沙箱内跑完；建议在本地终端补跑作为合并门槛。

### 13.5 待办（合并前）

1. 本地终端：`cargo fmt --all`、`cargo test --workspace --all-features --no-fail-fast`、`cargo build --release -p deepseek-desktop`（验证 winres / rc.exe 路径）。
2. 按第 9 节验收指标做端到端冒烟：
   - 连续开关 DS Pick × 5，DevTools 全程无 `ERR_CONNECTION_REFUSED`。
   - sidecar 内部插 20 s 阻塞 `/v1/sessions`，supervisor 不重启。
   - 恢复 500+ 消息会话，HTTP 立即 202，`/v1/tasks/{id}` 进度收敛至 `ready`。
   - `taskkill /F /IM deepseek-tui.exe`，supervisor < 1 s 重启。
3. 72 小时浸泡：`supervisor.log` 中 `event=restart reason=busy_timeout` 出现次数 = 0。

### 13.6 后续

- 阶段 5（SQLite 持久化层）保持独立立项，预计 1–2 周；启动条件：本治理 PR 合并 + 浸泡指标达标 ≥ 2 周。
- 仓库 pre-existing 的 56 个 clippy warning 建议另开清理 PR 处理（不与本治理混合，便于 review 与回滚）。

---

> 评审通过后，按"实施时间表"逐阶段进入开发；每个阶段独立 PR，独立验收。
