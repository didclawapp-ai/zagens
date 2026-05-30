# 交接：思维链中途截断调查（三通道并发争用）

**日期：** 2026-05-29
**性质：** 调查交接（新会话继续查，**重点**）。不是让你直接改，先把根因测实。
**一句话：** 编码任务首轮在 reasoning（思维链）流式过程中**突然截断、空正文、turn 仍标 `Completed`**；初步锁定为"每个思维 delta 同步落库 + 全局单锁背压"把引擎读流卡到上游超时。用户的核心调查思路：**思维链进行时，三个通道 A/B/C 同时在干什么、互相怎么争。**

---

## 0. 用户的核心思路（务必按这个主线查）

桌面端在一次 turn 的 reasoning 流式过程中，**三个通道并发运行**，要逐一查清"思维链那几十秒里，每个通道在做什么、是否互相争用/阻塞"：

- **通道 A — Tauri IPC**：密钥、系统设置、终端 PTY、文件。
- **通道 B — Runtime 代理**：Rust 注入 Bearer、转发 `/v1/*`、**SSE 流**（LLM 流就在这条上）。
- **通道 C — Sidecar 监督器**：启动子进程、**健康探测**、崩溃退出重启。

参考架构图/文档：`docs/tech/RUNTIME_ARCHITECTURE.md`（用户刚打开）。问题不一定只在 B，要看 **A/B/C 在思维链期间的并发交互**。

---

## 1. 现象（多轮复现，"基本上失败都是这个原因"）

- 首轮模型疯狂吐 `reasoning_content`（如 jqlite 的 lexer 正则表），**几千个 thinking delta**，然后**突然中断，没有可见正文**。
- turn 在 DB 里标 `Completed`（不是 Interrupted/Failed）。
- **无 UI 闪断、无重启**（用户确认；`supervisor.log` 也证实那时段无重启）。
- 控制台可见：`127.0.0.1:7878/health … ERR_CONNECTION_REFUSED` + `[TAURI] Couldn't find callback id … app reloaded while Rust running async operation`（**这两条的时间归属还没钉死**，可能是另一时刻/另一轮的残留，需在新会话里用带时间戳的日志对齐）。

## 2. 已排除

- **不是 sidecar 崩溃**：`C:\Users\Administrator\.zagens\logs\sidecar.log` 零 panic/fatal；当前 sidecar（pid 20636）连续 uptime ~43min 心跳正常。
- **不是 supervisor 重启**：`supervisor.log` 只有 `restart reason=config_change` 和 `shutdown action=killing_sidecar`，无 crash；失败轮（本地 20:38–20:44）时段内无任何重启。
- **不是 LHT 逻辑**：失败轮 `continue_injected=0`，gate 只发了 `graph_empty` 遥测（不进模型上下文）。LHT 是旁观者。**但**我们这次加的桌面侧 harness 实时轮询 + SSE 重连**会加剧**争锁（见下），属"放大既有脆弱点"。
- **不是标题生成并发 LLM 调用**：标题只是截断首条用户消息（`session_manager.rs:631`），非 LLM 调用。
- **不是真的压缩注入**：`compaction_enabled=false`、`should_compact=false`、`run_auto_compaction` 在 enabled=false 时直接 return；之前那次"空壳 Compaction Handoff"是模型读了系统提示里的 `compact.md` 模板后**脑补**的。

## 3. 初步根因链（强假设，**尚未直接测到阻塞时长**）

1. **引擎事件通道有界 256**：
   `crates/core/src/engine/runtime_new.rs:46` → `let (tx_event, rx_event) = mpsc::channel(256);`
2. **每个思维 delta 同步落库**：`monitor.rs` 收 `ThinkingDelta` → `mgr.emit_event(...).await?`（`monitor.rs:82-92`，inline 串行）→ `manager.rs:98 emit_event` → `persist.rs:364 append_event`。
3. **落库走全局单一连接锁**：`persist.rs:391-397` `spawn_blocking(move || append_event_sqlite(&db.lock().unwrap(), ...))`。
4. **SSE 读路径抢同一把锁**：`persist.rs:425-428 events_since` 也 `db.lock().unwrap()`；线程/item 保存、harness 任务图查询同样挤这把锁。

**推断的截断机制：**
```
LLM 吐 reasoning → 引擎 tx_event.send(ThinkingDelta).await（上限 256）
 → monitor 逐个消费，每条 spawn_blocking 抢全局 db 锁写 events
   → 写慢于吐（锁争用：SSE 读 / item 写 / harness 查同抢一锁）
     → 256 灌满 → 引擎 send 阻塞 → 引擎停止 .next() 读 HTTP 流
       → 上游 TCP 空闲 → idle 超时/重置 → reasoning 流中途截断
         → 引擎见流提前结束 → turn 空正文 Completed
```
吻合"思维越长越容易失败 / 无重启 / 空正文 Completed"；thr_0aea 的 **9283 条 item.delta** 即每 delta 落库的实证。

## 4. 三通道嫌疑点（新会话逐一查）

| 通道 | 思维链期间在做什么 | 重点查 |
|------|------|------|
| **A Tauri IPC** | 文件/PTY/设置等命令 | 是否有命令在 turn 中被前端反复调用、是否和 sidecar 写库争资源 |
| **B Runtime 代理** | 转发 `/v1/*`、**回传 SSE**；前端 `pollThreadTurnEventsViaTauriProxy` 读事件 | 代理转发 SSE 时是否走 `events_since`（争 db 锁）；引擎读上游流的超时设置；**这条最可能是截断现场** |
| **C Sidecar 监督器** | 健康探测 `/health`、pong 心跳 | 探测频率/超时；**思维链期间一次 `/health` 卡住会不会触发什么**；`crates/desktop/src/sidecar.rs`（:335 pong unbounded channel） |

## 5. 下一步（建议顺序）

1. **加计时埋点测铁证**：在 `monitor.rs` 消费循环量每条 `emit_event` 耗时 + 通道积压深度；在引擎 `tx_event.send` 处量 `.await` 阻塞时长。一轮 reasoning-heavy 任务就能看到"send 阻塞是否随 reasoning 增长飙升"。
2. **定位通道 B 代理**：找转发 `/v1/*` + SSE 的代理代码（Tauri 命令层，`crates/desktop/src/`），确认 SSE 回传是否经 `events_since`（争锁），以及引擎对上游 DeepSeek 流的 read/idle 超时。
3. **查通道 C 健康探测**：`crates/desktop/src/sidecar.rs` 健康探测频率/超时/失败动作。
4. **DB 并发模型**：确认 SQLite 是否开 WAL；全局 `Arc<Mutex<Connection>>` 是唯一连接吗（读写都挤它？）。

## 6. 修复方向（验证后再做，按性价比）

1. **思维 delta 不再每条同步落库**（最高杠杆）：reasoning 易失，UI 走广播通道即可；要么不持久化 thinking delta，要么**批量合并**（每 N ms/N 字一次），8000 次 INSERT → 几十次。
2. **持久化与引擎解耦**：monitor 快速排空 `rx_event` 到内存队列，独立任务批量落库，**引擎永不因 DB 背压停读流**。
3. **读写分离 / 开 WAL**：SSE 读用独立连接，别和写挤一把锁。
4. 兜底：调大 256（治标）。

## 7. 与已落地工作的关系（已 git 提交，commit `7455956` 之后还有改动）

- LHT「进展放行」漏洞已修（`nudge.rs prepare_nudge`：进展只清 streak 不跳 nudge，删了 `SkipProgressReset`）。
- LHT 诊断 `long_horizon.gate_skip` 已加（`mod.rs` + `no_tool_uses.rs`），可在 `runtime.db` 的 `events` 表查 reason。
- 这些都已编译+测试过、CHANGELOG/文档已记、已提交。**本次截断问题与上述 LHT 改动无逻辑关系**，但桌面侧实时轮询会加剧争锁。

## 8. 关键路径速查

| 文件 | 关注点 |
|------|------|
| `crates/core/src/engine/runtime_new.rs:46` | `tx_event` 有界 256（背压源） |
| `crates/runtime-orchestrator/src/runtime_threads/monitor.rs:82-92` | 每个 ThinkingDelta inline `emit_event` |
| `crates/runtime-orchestrator/src/runtime_threads/manager.rs:98` | `emit_event` → append_event + broadcast |
| `crates/runtime-orchestrator/src/runtime_threads/persist.rs:364-418` | `append_event`：spawn_blocking + 全局 `db.lock()` |
| `crates/runtime-orchestrator/src/runtime_threads/persist.rs:420-453` | `events_since`：SSE 读，争同一把锁 |
| `crates/desktop/src/sidecar.rs` | 通道 C：监督器/健康探测/pong（:335） |
| `docs/tech/RUNTIME_ARCHITECTURE.md` | A/B/C 三通道架构 |
| `C:\Users\Administrator\.zagens\logs\{sidecar,supervisor}.log` | 运行期证据（注意日志=本地时区，DB=UTC，差 8h） |
| `runtime.db`（`C:\Users\Administrator\.zagens\tasks\runtime\`） | 表 events/turns/threads；DEMO2 失败轮 `thr_0aea7dcc`、`thr_2f55`(一口气成功)、`thr_0eda`(早停) |

## 9. 可直接粘进新会话的摘要

> 桌面端编码任务首轮在 reasoning 流式中途截断、空正文、turn 标 Completed，多轮复现。已排除 sidecar 崩溃/supervisor 重启/LHT 逻辑/压缩。强假设：引擎事件通道有界 256（`runtime_new.rs:46`）+ 每个 thinking delta 同步落库（`monitor.rs:82` → `persist.rs:364 append_event`，spawn_blocking+全局 `db.lock()`）+ SSE 读 `events_since` 争同一把锁 → 通道灌满 → 引擎 send 阻塞 → 停读上游 HTTP 流 → idle 超时截断。用户主线：查**思维链期间通道 A(Tauri IPC)/B(Runtime 代理转发 /v1/*+SSE)/C(Sidecar 监督器健康探测) 三者并发在做什么、如何争用**。下一步：加计时埋点测引擎 send 阻塞时长 + 定位通道 B 代理的 SSE 转发与上游超时 + 查通道 C 健康探测。修复首选：thinking delta 批量/不落库 + 持久化与引擎解耦。详见 `.claude/stream-truncation-investigation-handoff.md`。

---

## 10. 第二轮调查结论（2026-05-29，静态代码已逐条核对 + 已加埋点）

**背压链已逐环证实（静态），机制成立：**

1. **背压源**：`runtime_new.rs:46` `mpsc::channel(256)` — 引擎事件通道有界 256。✔
2. **停读流的确切位置**：`streaming_phase.rs:396` 在**读流 `loop` 内部** `host.tx_event().send(Event::ThinkingDelta).await`。读流循环 `streaming_phase.rs:162-180` 的 `tokio::select! + timeout(chunk_timeout=90s, stream.next())` **只给 `stream.next()` 计时，不给事件处理里的 `send().await` 计时**。所以 send 阻塞期间 chunk_timeout/max_duration **都不触发**，循环卡在 send、不再读 socket。✔（这是最关键的一环，原假设未点明）
3. **空正文 Completed 的成因**：上游空闲 → 关连接 → 下次 `stream.next()` 返回 `Ok(None)`（"正常结束"）→ `break`。因 `current_thinking` 非空 → `stream_died_with_nothing=false`（`streaming_phase.rs:502-506`）→ **不触发透明重试** → turn 以空正文 `Completed` 收尾。完全吻合"思维越长越易失败 / 无重启 / 空正文 Completed"。✔
4. **写侧是主瓶颈**：`manager.rs:106-110 emit_event` 先 `append_event(...).await`（`persist.rs:391` spawn_blocking + 全局 `db.lock()` 写）**再**广播 → monitor 排空循环被每条 delta 的 DB 写串行卡住。9283 条 delta = 9283 次串行 INSERT。✔

**对原假设的修正：**
- **SSE 读不是轮询 `events_since`**。`runtime_api/stream.rs:345-417` 只读一次 backlog，之后订阅 **broadcast 广播通道**（`EVENT_CHANNEL_CAPACITY=4096`，`manager.rs:26`）收 live delta；**只有 `RecvError::Lagged` 时**才 `events_since_async` catch-up（`stream.rs:395-401`）。read 争锁是**滞后放大器**而非持续轮询：reasoning-heavy 轮（>4096 delta）撑爆广播 → Lagged → catch-up 读抢 `db.lock` → 写更慢 → monitor 更慢 → 恶性循环。
- **SQLite 已开 WAL + synchronous=NORMAL**（`thread_store_sqlite.rs:109`），fsync 不在路径上。**但全局只有一个 `Arc<Mutex<Connection>>`**（`persist.rs:39`），WAL 只对**多连接**有意义，单连接 + 单 mutex 下读写仍全部串行 → 瓶颈仍在。

**三通道结论：**
- **通道 A（Tauri IPC）**：文件/PTY/设置命令在桌面壳层，不碰 sidecar 的 runtime.db，**非争用方**。
- **通道 B（Runtime 代理）**：`runtime_proxy.rs` 的 `runtime_post_stream`（LLM 流中继，3600s）/`runtime_get_sse`（事件中继，3600s）只转发字节；真正争 `db.lock` 的 `events_since` 在 **sidecar 进程内**、与引擎/monitor 同进程。**截断现场在 sidecar 内的写侧背压，不在桌面代理**。
- **通道 C（监督器）**：健康探测**主走 stdin/stdout ping/pong**（`sidecar.rs:627`，注释明说独立于 axum HTTP 栈），1s 超时失败才回退 HTTP `/internal/probe`（3s）；`MAX_BUSY_TIMEOUTS=12 × 5s = 60s` 才重启。**解释"失败轮无重启"，旁观者**；其健康只证明 sidecar 进程没死。

**已加埋点（本次，diagnostic-only，已编译通过）：**
- `monitor.rs` ThinkingDelta 臂：测每条 `emit_event` 耗时 + 读通道积压 `rx.len()`；`emit_ms≥50` 或 `rx_backlog≥200` 时 `warn!`，每 500 条 `info!` 聚合（avg/max/backlog）。
- `streaming_phase.rs` ThinkingDelta send：测 `tx_event().send().await` 阻塞时长，`≥50ms` 时 `warn!`。

**如何取铁证（下一步运行）：** 跑一个 reasoning-heavy 编码任务复现；看 sidecar.log：
- 若 `engine ThinkingDelta send blocked ...` 的 `send_ms` 随 reasoning 推进飙升、且 monitor 侧 `emit_ms`/`rx_backlog` 同步走高 → 背压链坐实。
- 关注截断时刻前最后一条 `send_ms` 是否接近"上游空闲关连接"的时间窗。

**修复方向（验证后做，按性价比）：**
1. thinking delta **不再每条同步落库**（最高杠杆）：UI 走广播即可，thinking 改**批量合并**（每 N ms / N 字）或不持久化 → 8000+ INSERT 降到几十次。
2. **持久化与引擎解耦**：monitor 快速排空 `rx_event` 到内存队列，独立任务批量落库，**引擎永不因 DB 背压停读流**。
3. （治本之一）让读流循环的 idle 看门狗也覆盖事件处理/send 阶段，或给 `tx_event.send` 加超时降级（丢弃/合并 thinking delta 而非阻塞读流）。
4. 读写分离 / 多连接（WAL 才真正发挥作用）。

---

## 11. 第三轮：复现日志判读 + 修复落地（2026-05-29 21:40）

**21:27–21:31 复现轮日志判读（`~/.zagens/logs`）：**
- sidecar pid 18748、v0.8.15，**日志里 0 条埋点输出**（`send_ms`/`emit_ms`/`rx_backlog` 全无）→ 跑的是**旧二进制**，埋点没编进去，光等日志拿不到铁证。
- 但画像吻合预测：线程 `thr_0aeaad49`（DEMO2，reasoning-heavy 的 jqlite lexer 任务）、pong 心跳全程规律（uptime 一路到 270s）、supervisor **无任何重启**。心跳走 stdin/stdout（纯异步、不碰 db 锁）→ 即使 db 写路径被 `db.lock()` 串行堵死、心跳照常，正好印证"卡的是阻塞写路径，不是整个 runtime"。

**已落地修复（最高杠杆 = 修复方向 #1，并部分达成 #2）：**
- `monitor.rs`：reasoning delta **合并落库**——`thinking_buffer` 累积，按 `THINKING_FLUSH_BYTES=512` / `THINKING_FLUSH_INTERVAL=60ms` 阈值才 `flush_thinking_buffer`（一次 `emit_event`）。常路径变成 O(1) 字符串追加，monitor 飞快排空 256 通道 → 引擎不再因 DB 背压卡 `tx_event.send`。`ThinkingComplete` 与循环结束后各补一次 flush（截断时保留残余 reasoning）。~9283 次 INSERT → 几十次。
- 埋点保留（改为按 flush 计量），用于回归监控。
- `streaming_phase.rs` 的 `send_ms` 埋点保留（引擎侧证据）。
- 编译 + `cargo test -p deepseek-runtime-orchestrator`（16 passed）通过；release sidecar 已构建：`target/release/zagens-runtime.exe`（21:40）。

**部署（需用户操作才能让运行的 app 生效）：** 旧 binary 仍在以下位置（21:19 构建）：
`crates/desktop/binaries/zagens-runtime-x86_64-pc-windows-msvc.exe`、`%LOCALAPPDATA%\Zagens\zagens-runtime.exe`、`%TEMP%\zagens-runtime.exe`。把 `target/release/zagens-runtime.exe` 覆盖到 app 实际加载的那个（取决于启动方式），或重建桌面 app 重新打包 sidecar，然后重启 + 复现。

**复现验证要点：** 修复后 sidecar.log 里 `rx_backlog` 应稳定接近 0、`coalesced thinking flush` 几乎不报 `warn!`；引擎侧 `ThinkingDelta send blocked` 不再出现；reasoning-heavy 任务能跑到正文/工具调用而非空正文 Completed。

**仍可做（后续，非必须）：** #2 完整解耦（独立落库任务）、#3 读流 idle 看门狗覆盖 send 阶段、#4 读写分离/多连接让 WAL 真正生效。

---

## 12. 第四轮：埋点不可见的根因 + 改用 eprintln（2026-05-29 22:01）

**关键坑：sidecar 没有 tracing 订阅器。** 替换新二进制后复现仍截断（线程 `thr_77a78758`，pid 24240，21:46 启动、确认是新 binary），但 sidecar.log 里**任何 tracing 输出都没有**——不只是我的埋点，连既有的 `monitor_turn start`、streaming_phase 的 `Tool block start` 等 `tracing::info!` 也全无。排查发现：**整个 workspace 没有 `tracing-subscriber` 依赖、没有任何 subscriber 初始化**（grep 全仓零命中）。没有 subscriber，所有 `tracing::*` 事件在运行时被静默丢弃。日志里只有 `eprintln!`（`[deepseek-runtime]`）/`println!`（`DS_PICK_*`）能出现。

→ **原 handoff「用 tracing 加埋点测铁证」的方案在 sidecar 部署下根本看不到。** 已把全部探针改为 `eprintln!`（直达 stderr → sidecar.log）：
- `streaming_phase.rs`：(a) `[stream-probe] engine ThinkingDelta send blocked <ms>ms`（send 背压）；(b) `[stream-probe] chunk_timeout ...`；(c) **最关键**——循环结束后 `[stream-probe] stream ended reason=<upstream_eof|chunk_timeout|cancelled|stream_event_break> elapsed_ms=… thinking_bytes=… text_bytes=… tool_uses=… stream_errors=… pending_msg=… transparent_retries=…`。
- `monitor.rs`：`[thinking-probe] SLOW flush …` / `[thinking-probe] stats …`（每 100 flush）/ `[thinking-probe] turn done deltas=… flushes=… max_emit_ms=…`。

**注意：** pong 心跳全程规律、supervisor 无重启（与前几轮一致，通道 C 仍旁观者）。DevTools 的 `ERR_CONNECTION_REFUSED /health` + `[TAURI] app reloaded while Rust is running async operation` 是 **WebView reload** 时的前端瞬态（reload 撞上 sidecar 重启窗口），与 sidecar 内 turn 截断不是同一层；需用新 eprintln 探针确认 sidecar 侧 `stream ended reason`。

**下一步（关键判读）：** 重启 + 复现后看 sidecar.log：
- `reason=upstream_eof` 且 `thinking_bytes>0`、`text_bytes=0`、`tool_uses=0` → 仍是上游空闲关流的空正文截断；再看是否伴随 `send blocked`/`SLOW flush`（若**没有**背压告警却仍 eof，说明截断另有原因，**不是**写侧背压——需转向上游 HTTP 客户端的 read/idle 超时或 DeepSeek 侧）。
- `reason=chunk_timeout` → 90s 内无 chunk，偏上游/网络。
- `deltas` vs `flushes` 比值确认合并是否生效（应几十:1）。

---

## 13. 第五轮：铁证到手，根因不是背压（2026-05-29 22:16）

**eprintln 探针生效，拿到决定性数据（线程 `thr_50bd8db5`，turn `turn_e6637ebb`）：**
```
[thinking-probe] stats … rx_backlog=0..8 avg_ms=0 max_ms=7   （整轮，从 deltas=388 到 8063）
[stream-probe] stream ended reason=upstream_eof elapsed_ms=162403 thinking_bytes=28931 text_bytes=0 tool_uses=0 stream_errors=0 pending_msg=false transparent_retries=0
[thinking-probe] turn done deltas=8189 flushes=2038 max_emit_ms=32
```

**两个铁结论：**
1. **背压修复完全生效**：整轮 `rx_backlog` 0~8、`emit_ms` 几乎 0（max 7，收尾 32）、**零** `send blocked` / `SLOW flush`。8189 deltas → 2038 flushes（~4:1）。**写侧背压已不存在。**
2. **截断根因 ≠ 背压**：`reason=upstream_eof`（DeepSeek 干净关流，`stream_errors=0` 非网络错）、模型连吐 reasoning **162 秒 / 28931 字节 / 约 8K reasoning tokens**、**正文 0 字节、工具 0 个**，然后上游关流。即"模型一直在思考、始终没产出答案/工具调用，上游到点把流关了"。

**已排除：** `max_tokens`=65536（`effective_max_output_tokens` for V4 Pro）、reasoning 才 ~8K tokens，**远没到 token 上限**；DeepSeek 客户端 idle 超时 300s（162s 没触发）；引擎 chunk_timeout 90s（chunk 一直在流，没触发）。

**最后一块拼图（本轮已加探针，待复现）：** streaming_phase 的 `MessageDelta` 处理**原来丢弃了 stop_reason**（只取 usage）——已修复为记录 `last_stop_reason`，并把 `stop_reason` + `out_tokens` + `max_tokens` 加进 `[stream-probe] stream ended` 行。下次复现即可判定：
- `stop_reason="length"` → 命中某 token 上限（但若 out_tokens 仍 << max_tokens，则是 DeepSeek 对 reasoning 的独立上限或服务端策略）。
- `stop_reason="stop"` → 模型"自认为答完"却零正文（模型行为异常 / 被内容策略拦截）。
- `stop_reason=None` + `reason=upstream_eof` → **纯基础设施/时长切断**（~162s 像服务端或中间代理的单请求时长上限），与 token 无关。

**修复方向（待 stop_reason 判定后定）：**
- 若 `length`/token 上限：调大 max_tokens 无用（reasoning 另算），需在请求里**降 `reasoning_effort`**（让模型早点收敛）或限制思考预算。
- 若 `None`/时长切断：对"reason=upstream_eof 且零正文零工具"做**透明重试**（当前 `stream_died_with_nothing` 要求 `stream_errors>0` 且 thinking 为空，命中不了——需放宽），或缩短单请求以避开上游时长帽。
- 若 `stop`：排查 system prompt / reasoning_effort 让模型陷入"只想不答"。

**注意：背压合并修复（§11）保留**——它消除了真实的潜在隐患，且日志证明 backlog 已归零；只是它不是本案截断的元凶。
