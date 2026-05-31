# CodeCrafters Redis — 单模型串行长程生成

**案例编号:** REDIS-SERIAL
**所属:** [`../LHT_TEST_SUITE.md` §3/§6](../LHT_TEST_SUITE.md)（外部经典案例 / 最小回归集）
**角色:** **单模型串行基线** —— 验「长程 + 客观验收」，并作为 [`../PARALLEL_FRESH_GENERATION.md`](../PARALLEL_FRESH_GENERATION.md) §7.1 去风险实验「量痛点」的串行对照（先有串行墙钟/成本数,才谈并行收益)。
**钓鱼点:** 协议即天然契约(RESP),命令逐级叠加 → 压 step/context 阈值;验收能用**真实 `redis-cli`** 做判定式 oracle,堵死「实现了 ≠ 跑通」假绿。

---

## 1. 可复制 Prompt（直接粘贴给 runtime）

```
用 Rust 从零实现一个兼容 Redis 协议的服务器（不要用任何 Redis 客户端/服务端库，TCP + 手写 RESP 解析）。串行完成、不要 spawn 子代理。按下面阶段逐步实现，每个阶段都要能用真实 redis-cli 连接验证通过：

1. 监听 127.0.0.1:6379，接受 TCP 连接；
2. 正确解析 RESP（数组/批量字符串），响应 PING → +PONG；
3. 支持单连接内多条命令、支持多个并发客户端（每连接一个任务/线程，互不阻塞）；
4. ECHO <msg> → 原样返回；
5. SET key value / GET key（不存在返回 nil）；
6. SET 的 PX 过期：SET key value PX 100 后，100ms 内 GET 命中、之后返回 nil；
7. CONFIG GET dir / dbfilename（返回启动参数，缺省给合理默认）；
8. KEYS *（返回当前所有键）；
9. INFO replication（至少返回 role:master）。

工程要求：
- 用 cargo 工程结构，命令分发清晰可扩展；
- 提供 scripts/test_redis.sh：启动服务器，用 redis-cli 跑通上面每个阶段的断言（任一断言失败整体非零退出），结束后关停服务器；
- 错误命令返回 RESP 错误而非 panic；不得 unwrap 在连接处理路径上崩整个进程。

完成标准（必须用 [verify:] 写进 checklist 并真实跑过）：
cargo build 通过、cargo clippy 无警告、cargo test 通过、bash scripts/test_redis.sh 全绿。
示例脚本「创建」不算完成，必须「跑通」。
```

> **可换语言:** 把「用 Rust」换成 Go / Python / TypeScript 即可;`[verify:]` 命令相应换成 `go build/test`、`pytest`、`npm test`。RESP 协议与 `redis-cli` oracle 不变。

---

## 2. 期望的 `[verify:]` checklist（健康产物长这样）

```
[ ] TCP 监听 6379 + RESP 解析（数组/批量字符串）
[ ] PING/PONG
[ ] 并发客户端（多连接互不阻塞）
[ ] ECHO
[ ] SET / GET（含 nil 语义）
[ ] SET ... PX 过期
[ ] CONFIG GET / KEYS / INFO replication
[verify: cargo build]                编译通过
[verify: cargo clippy -- -D warnings] 无警告
[verify: cargo test]                 单测通过
[verify: bash scripts/test_redis.sh] redis-cli 全阶段断言跑通   ← 验收锚点
```

**红线:** 出现「创建 test_redis.sh」这类无 `[verify:]` 完成项当验收 → 触发 `unverified_acceptance_suffix`(同 DEMO3 教训)。

---

## 3. 验收 oracle（真实 redis-cli 做判定）

`scripts/test_redis.sh` 的核心断言(人/CI 侧也可直接照跑;需本机有 `redis-cli`):

```bash
set -euo pipefail
# 后台启动被测服务器（按产物实际启动方式替换）
<启动命令> & SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
sleep 0.5
R="redis-cli -p 6379"

test "$($R PING)" = "PONG"
test "$($R ECHO hello)" = "hello"
$R SET foo bar >/dev/null
test "$($R GET foo)" = "bar"
test "$($R GET missing)" = ""                       # nil → 空串
$R SET t v PX 100 >/dev/null
test "$($R GET t)" = "v"
sleep 0.2
test "$($R GET t)" = ""                              # 过期后 nil
$R INFO replication | grep -q "role:master"
echo "ALL REDIS STAGES PASSED"
```

> 没有本机 `redis-cli` 时,可用 CodeCrafters 官方 `codecrafters test`(按其 README 配置)替代;但**回归基线推荐用 `redis-cli`**,无需注册外部账户、可离线 CI。

---

## 4. 离线回放与判定

```powershell
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\]'
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[stream-probe\]'
```

| 维度 | 通过 |
|------|------|
| 验收锚点 | 「redis-cli 全阶段跑通」带 `[verify: bash scripts/test_redis.sh]` |
| `verify_gate` | 4 项 `[verify:]` 全 `verified`（无 `untagged_ok`/`mismatch`） |
| 实跑 | `test_redis.sh` exit 0：PING/ECHO/SET/GET/PX 过期/INFO 全断言过 |
| 进度诚实性 | 全勾 ⇔ oracle exit 0 |
| 长程信号 | 若撞 step/context/cycle 阈值,节点流应出现 `step_limit_continue`/`context_warning`/`cycle_advanced` 而**非**孤立 `incomplete_stop` |

**作为并行实验对照:** 记录本串行 run 的**墙钟时间 + 总 token**,供 [`../PARALLEL_FRESH_GENERATION.md` §7.1](../PARALLEL_FRESH_GENERATION.md) 实验 1「量痛点」与实验 2「手动 fan-out」对比——串行不够痛则并行不该做。

---

## 5. 2026-05-31 首跑实证(钓出 `exec_shell` 挂死出口)

Windows 环境首跑(`F:\LHT_TEST\CodeCrafters_Redis`),实现侧质量不错(`cargo build`/`clippy -D warnings`/`test` 29/29 全过、RESP/命令/PX 过期/并发均实现),但暴露了**两个环境/harness 层问题**,远比验收闭环本身更值钱:

1. **环境缺口:** 本机无 `redis-cli`、无可用通用 `bash`(仅 `docker-desktop` WSL,无常规 bash) → 验收锚点 `[verify: bash scripts/test_redis.sh]` 在本机**无法原样跑**。模型适应力强:`winget install Redis.Redis` 自行装来 `redis-cli`(理想行为,非打桩造假)。**副作用:** winget 的 Redis 包注册并启动了一个真 Redis 服务抢占 6379,模型 `netstat`+`Stop-Process` 自行处理了冲突(短暂污染,未致假绿)。
2. **`exec_shell` 永久挂死(已修):** 模型用 `Start-Process -NoNewWindow` 前台启动自己的 `redis-server.exe`,该子进程**继承了 exec_shell 的 stdout 管道写端**;server 常驻不退 → 管道永不 EOF → runtime 的 reader 线程 `join()` 永久阻塞 → `poll()`/前台超时循环卡死、**`timeout_ms` 完全失效**,turn 冻死 19+ 分钟。根因与修复(`collect_output` 改有界 join + detach)见 [`../../CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]` Runtime 首条。**这是 length 截断 / prose 早停 / step 耗尽 / loop-guard 之外的第 5 类静默卡死出口**,且专属"长程任务起常驻服务"场景——DEMO3(纯批处理)碰不到,CCR(协议服务器)必然踩中。

**回归启示:** 凡验收涉及"起服务再连"的案例(Redis/HTTP server/DB),务必同时盯 **exec_shell 是否因继承管道挂死** + **收尾是否残留孤儿服务进程**(Windows 侧 `child.kill()` 不杀子树,待 Job Object follow-up)。

---

## 6. 2026-05-31 复跑实证(钓出 UI 进度/checklist 与引擎真值永久分叉)

`exec_shell` 挂死修好后复跑同一 prompt,实现侧再次很好(RESP/命令/PX 全实现、`cargo build`/`clippy`/`test` 全绿),但暴露一处 **UI↔引擎状态分叉**:

- **现象:** 12 项 checklist 模型**全部完成**,但桌面进度条 / 清单**永久卡在 7/12 = 58%**、item 8-12 显示未完成;节点流却显示 `verify_gate` item 1-12 全 fire、末尾 `gate_skip reason=graph_complete open_items=0`——即**引擎 `todos` store 是 12/12 真完成**,UI 落后 5 项。
- **取证(DB `thr_7eb88089`):** 持久化的 `threads.checklist_json` = 12 项 / 完成 7 / pct 58 / item9=in_progress;`items` 表仅记录 **3 次** checklist 工具调用(`write` 0% → `write` 58% → `update` item9→in_progress),此后把 item 8-12 标完成的变更**既无 item 记录也未 persist**,而同期 exec_shell/edit_file 等工具均正常记录。
- **根因:** UI 读取的持久化 checklist 仅由 monitor 逐工具 `ToolCallComplete` 钩子写入,且被 `tool_items.remove(&id)`(start/complete 配对)门控;并行 / deferred 批次里的 `checklist_update` 一旦 start 没进 `tool_items`,完成即被丢弃,持久化快照与引擎真值**永久分叉**(无收尾对账)。这是 **continue/verify 信号正常、但"产物呈现层"骗人**的新一类问题——和"假绿"相反:**引擎真完成,UI 假未完成**。
- **修复(已落地):** 引擎每次成功 checklist 变更经可靠 harness-status 通道推权威 `TodoListSnapshot`,host 直接对账持久化 + 重推面板。详见 [`../../CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]` Runtime 首条。
- **残留:** item 12 `[verify: bash scripts/test_redis.sh]` 仍 `verify_gate=mismatch`(复合命令未匹配到执行),属既有 `mismatch` 漏标洞(B 只阻断 `unverified_acceptance`),另列下一锤候选。

**回归启示(补):** "进度/清单不动但任务实际跑完了"未必是模型早停——先核 `gate_skip open_items` 与引擎 `todos` 真值,再判定是 UI 呈现分叉还是真未完成;凡 UI 状态来自"逐事件累积"而非"权威快照对账"的通道,都要有收尾对账兜底。

---

## 7. 2026-05-31 三跑实证(首个全链路 clean pass + 全程可追溯)

`exec_shell` 挂死(§5)与 UI/引擎分叉(§6)修复后,第三次复跑(产物 `F:\LHT_TEST\CodeCrafters_Redis01`,thread `thr_734ba23a`)首次取得**全链路真绿且可追溯**,§5/§6 历史回归全部清零。

**完成判定(引擎真值 + 独立复核双重确认):**

- **节点流:** item 12 `[verify: cargo build]`、13 `[verify: cargo clippy]`、14 `[verify: cargo test] (10/10)`、15 `[verify: bash scripts/test_redis.sh] (16/16)` 四项 verdict 全 `verified`;item 1–11 实现项为 `untagged_ok`(无 `[verify:]` 标签,符合 §2 设计)。末尾 `gate_skip reason=graph_complete open_items=0 incomplete=false`——干净收尾,无 false `incomplete_stop`。
- **verify 拦截实证有效(先红后绿):** 首次 `bash scripts/test_redis.sh` **失败**(exit 1)——客户端断开时服务器刷 `os error 10054` parse-error、`KEYS *` 的 bash glob 扩展、ECHO 带空格引号;模型据此自愈(连接断开优雅退出 + 脚本引号/glob 修正),**重跑才 16/16 全绿**(thread `item_f4d9d009`,Phase 2–10 全 PASS)。即"实现了 ≠ 跑通"这道线真的拦了一次,非一把过假绿。
- **独立复核(离线,不信 UI):** 在产物目录直接 `cargo build` ✅ / `cargo clippy -- -D warnings` 零警告 / `cargo test` **10/10**;本机 redis-cli 不在 PATH、WSL bash 不可用时,改用**裸 TCP 发 RESP** 复刻脚本断言,**13/13 全过**(PING/ECHO/SET·GET·nil/PX 过期前后/CONFIG GET dir·dbfilename/KEYS/INFO role:master/错误命令)。该探针留存 `scripts/resp_smoke.ps1`,作 Windows 侧无 redis-cli 的离线 oracle。

**验收依赖自筹(复用,非本次下载):** 本次 run 线程**无任何下载/安装动作**(`winget`/`git`/`Invoke-WebRequest` 零命中),复用了**首跑(§5)期间模型自筹的** `C:\Program Files\Redis\redis-cli.exe`(其目录创建时间早于本 run 约 8h 佐证)+ Git Bash 跑 `.sh`。首跑的自筹方式(包管理器 vs 源码下载)属 §5 范畴,本次未重新核定。要点:模型在缺验收 oracle 依赖时**自筹真实依赖、未打桩造假**。

**§5/§6 回归清零:**

| 历史坑 | 本次 |
|--------|------|
| §5 `exec_shell` 继承管道挂死 | 未复现(stream 全 `stream_errors=0`,无长时冻死) |
| §6 UI 卡 7/12 vs 引擎 12/12 | 已修生效(`checklist_persist` 多次回写,UI=引擎真值) |
| §6 残留 `verify_gate=mismatch`(bash 行漏标) | **消失**,item 15=`verified` |

**串行基线(供 [`../PARALLEL_FRESH_GENERATION.md` §7.1](../PARALLEL_FRESH_GENERATION.md) 对照):** 墙钟 ≈ **10 分钟**(`00:29:49Z → 00:39:57Z`,单 turn);output **29,379** tok、reasoning 12,582 tok、累计 input 1.80M(prompt-cache 命中 ~73%)。

**结论:** CCR 由"钓 bug 案例"转为**干净回归基线**,本案例视为**已毕业**;后续加压改用 **≥1H 级**长程任务(见 [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md))。

---

**修订记录:**
- 2026-05-30 创建:CodeCrafters Redis 单模型串行规格(可复制 prompt + `[verify:]` + redis-cli oracle + 串行基线测量)。
- 2026-05-31 补 §5:首跑实证——环境缺口(无 redis-cli/bash,模型 winget 自装)+ 钓出 `exec_shell` 继承管道永久挂死(超时失效)第 5 类出口,已修(`collect_output` 有界 join)。
- 2026-05-31 补 §6:复跑实证——UI 进度/checklist 卡 7/12 而引擎 12/12 真完成,根因为持久化只靠 monitor 逐工具事件、漏配即永久分叉,已修(引擎经 status 通道回写权威快照对账)。
- 2026-05-31 补 §7:三跑首个全链路 clean pass(`thr_734ba23a`)——四 `[verify:]` 门全 `verified`、先红后绿拦截实证、独立裸-RESP 复测 13/13、§5/§6 回归清零;记串行基线(墙钟 ~10min / output 29.4k tok / cache 73%),案例**毕业**,加压转 ≥1H 长程任务。
