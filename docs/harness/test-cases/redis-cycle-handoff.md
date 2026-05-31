# Redis 进阶 — Cycle 触发 + 跨 Cycle 交接保真（首测）

**案例编号:** REDIS-CYCLE
**所属:** [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md)（长程压力 / cycle 首测）
**角色:** **cycle 三支柱首测** —— 验「换脑（cycle_advanced）」+「交接（carry_forward + 结构化状态）保真」，是 [`codecrafters-redis.md`](./codecrafters-redis.md)（毕业的串行基线）的**加压续作**。
**与 CCR 的差异:** CCR 单 turn ~10min、活跃 input 远不到阈值、**从未触发 cycle**；本案例靠**大幅扩阶段数 + 重产出/重验证**把活跃上下文自然顶到 768K 阈值（**不下调阈值**，保留真实压力意义；多小时量级），再验交接后模型有没有「忘了要干什么 / 把摘要当原文乱编」。注意：能否自然触发取决于软容量护栏是否先截断（见 §0 校准）。
**钓鱼点:** cycle 是「半原文半摘要的 Frankenstein 上下文」高发区（`cycle_manager.rs` §Why）。本案例埋一条**只在开头声明一次、要贯穿到最后**的横切约束（见 §3 探针），cycle 一旦丢状态，末段产物必然违反它 → 被 oracle 抓到。

---

## 0. ⚠️ 触发条件 + 一个开跑前必做的校准（**不改阈值，靠真实规模顶上去**）

> 决策：**不下调 cycle 阈值**——下调即失去压力测试意义。本案例靠真实的超大规模把活跃上下文自然顶到阈值。

**触发逻辑**（`cycle_manager.rs:88-106` `should_advance_cycle`，逐行核实）：
```
threshold     = cfg.threshold_for(model)            // 默认 768000；现可经 [context] cycle_threshold 配（见下）
trigger_floor = min(threshold, window − headroom)   // headroom = 262144 + 1024 = 263168
触发 ⇔ active_input_tokens >= trigger_floor          // v4-pro 1M 窗 + 默认阈值 → trigger_floor = 768K（≈77%）
```

**本案例分两段跑（互不干扰）：**

**① 验证跑（先做，几十分钟，证明"到阈值就触发"是准的）。** 阈值已可配（见下方"缺口已修"），临时压低做一次小规模验证：
```toml
# ~/.deepseek/config.toml — 验证跑专用，验证完删掉
[context.per_model.deepseek-v4-pro]
cycle_threshold = 120000     # trigger_floor = min(120K, 785K) = 120K，1H 内可达多次
```
跑本案例前几个阶段即可，目标：节点流出现 **≥2 次 `cycle_advanced`**、且交接探针（§3 WCOUNT 往返）仍绿。**这不改默认、不污染压力测试语义**，只为坐实触发链路正确。

**② 压力跑（验证通过后，恢复默认 768K，不改阈值）。** 靠真实超大规模把活跃输入自然顶到 768K（窗口 77%）。参照实测：74 条消息 ≈ 109K（11%），即需 ~7× 规模、**~500+ agentic 轮 / 多小时**。阶段要足够多、每阶段产出与验证足够重（大 redis-cli 输出 + 反复读源文件），让 transcript 单调长大。前置：`[compaction] auto_compact = false`（默认即关），否则压缩会把 transcript 砍小、永远到不了阈值。

**⚠️ 开跑前必做校准（否则可能白跑数小时）：** cycle 在源码里是"a rare overflow safety net"。在它之前有更软的容量护栏会先动手——`capacity_flow/interventions.rs` 的 `TargetedContextRefresh` / `VerifyWithToolReplay` / `trim_oldest_messages_to_budget`。**若它们在 768K 之前就裁老消息/刷新，活跃上下文会被按在 77% 以下，cycle 永不触发。**
- 校准做法：开「长程任务 → 上下文」tab，跑前 ~30–60 分钟盯「当前 %」。
  - 若稳定爬升、逼近 75% → 规模路线成立，继续跑到 cycle。
  - 若早早 **plateau 在某个 < 75% 的值**（如卡在 50–60% 不再涨）→ **说明软护栏先截断了**，自然 cycle 当前不可达。**这本身就是本次最有价值的结论**：记录 plateau 值 + 当时触发的 `GuardrailAction`，作为"cycle 在真实负载下被软护栏抢先"的证据，无需再耗时间硬顶。
- 面板刷新节奏（已修，本轮）：`panel.context` 现在除了 MessageComplete / 回合末，还在**每个 per-step 安全边界**由引擎主动推 live 快照（绕开 mid-turn 饿死的 op loop），故长 turn 内压力条**每步都会更新**、不再冻到回合末；仅在**单个长工具**（cargo build/test 那几分钟、未跨步）执行期间不动属正常。

> **缺口已修（2026-05-31，本轮）：** 此前 cycle 阈值对用户**完全不可配**——`should_advance_cycle` 读的 `self.config.cycle` 在 `engine_spawn.rs` 恒为 `CycleConfig::default()`，`[context] cycle_threshold` 只喂 `SeamManager`，`[cycle.per_model]` 从未被反序列化。已新增 `Config::cycle_runtime_config(model)` 把 `[context] cycle_threshold`（全局）与 `[context.per_model.<model>] cycle_threshold`（按模型）真正接进 `engine_spawn` 的 `CycleConfig`，**默认 768K 不变**。这正是上面"验证跑"能成立的前提。详见 `CHANGELOG.md`。

---

## 1. 可复制 Prompt（直接粘贴给 runtime）

```
用 Rust 从零实现一个兼容 Redis 协议的服务器（不要用任何 Redis 客户端/服务端库，TCP + 手写 RESP 解析）。串行完成、不要 spawn 子代理。这是一个长任务，按下面阶段逐级实现，每个阶段都要能用真实 redis-cli 连接验证通过，且后一阶段不得破坏前面已通过的阶段。

【贯穿全程的硬约束（务必从第一阶段就实现，并保持到最后一个阶段）】
- 维护一个全局写计数器 op_seq：每执行一条“写命令”（SET/DEL/RPUSH/LPUSH/HSET/ZADD/EXPIRE/MULTI 提交的写、RDB 载入不计）op_seq += 1；
- 自定义命令 WCOUNT 返回当前 op_seq（RESP 整数）；
- op_seq 必须能随 RDB 持久化保存、并在重启载入后恢复（见阶段 G）；
- 所有错误回复统一以 `ERR ` 前缀开头，绝不 panic / 不 unwrap 在连接路径上。

【阶段】
A. 监听 127.0.0.1:6379，RESP 数组/批量字符串解析，PING→+PONG，ECHO；
B. 多并发客户端（每连接一线程，互不阻塞）；
C. SET/GET（不存在返回 nil）、DEL；SET 的 PX 毫秒过期 + EXPIRE/TTL/PERSIST；
D. 列表：RPUSH/LPUSH/LRANGE/LLEN/LPOP/RPOP；
E. 哈希：HSET/HGET/HGETALL/HDEL/HLEN；
F. 有序集合：ZADD/ZRANGE（含 WITHSCORES）/ZSCORE/ZCARD；
G. RDB 持久化：SAVE 写出快照文件（含全部键 + op_seq），启动时若存在快照则载入；CONFIG GET dir/dbfilename；
H. 事务：MULTI/EXEC/DISCARD（EXEC 原子执行入队命令）；
I. 发布订阅：SUBSCRIBE/PUBLISH（至少单频道，消息推送给订阅连接）；
J. KEYS *、DBSIZE、INFO replication（role:master）、WCOUNT。

【工程要求】
- cargo 工程结构，命令分发清晰可扩展，模块拆分合理；
- 提供 scripts/test_redis.sh：启动服务器，用 redis-cli 跑通每个阶段的断言（任一失败整体非零退出），覆盖到 WCOUNT 持久化往返（写若干 → 记 WCOUNT → SAVE → 重启 → 校验 WCOUNT 恢复一致）；结束关停服务器。

【完成标准（必须用 [verify:] 写进 checklist 并真实跑过，不得把“创建脚本”当“跑通”）】
cargo build 通过、cargo clippy 无警告、cargo test 通过、bash scripts/test_redis.sh 全绿（含 op_seq/WCOUNT 持久化往返断言）。
```

---

## 2. 期望的 `[verify:]` checklist（健康产物长这样）

```
[ ] A 监听+RESP+PING/ECHO     [ ] B 并发        [ ] C SET/GET/DEL/PX/EXPIRE/TTL
[ ] D 列表    [ ] E 哈希    [ ] F 有序集合    [ ] G RDB 持久化(含 op_seq)    [ ] H 事务
[ ] I 发布订阅    [ ] J KEYS/DBSIZE/INFO/WCOUNT
[verify: cargo build]                 编译通过
[verify: cargo clippy -- -D warnings] 无警告
[verify: cargo test]                  单测通过
[verify: bash scripts/test_redis.sh]  redis-cli 全阶段 + WCOUNT 持久化往返跑通  ← 验收锚点
```

---

## 3. 交接保真探针（cycle 专属，本案例核心）

横切约束 **op_seq / WCOUNT + 持久化往返** 是故意设计的「跨 cycle 记忆探针」：它在 prompt 开头**只声明一次**，却要贯穿到阶段 G（持久化）与末段脚本断言。

- **若交接保真**：无论中途换脑几次，末段 RDB 仍正确保存/恢复 op_seq，`WCOUNT` 往返一致 → oracle 绿。
- **若交接丢状态**（cycle 把这条横切约束摘掉/改写）：末段大概率出现「op_seq 不持久化」「重启后 WCOUNT 归零」「错误前缀漂移成非 `ERR `」等 → oracle 红。

这比「整体能不能跑」更尖锐：它把**交接是否丢了开头的全局约束**变成一个 redis-cli 可判定的布尔。

---

## 4. 验收 oracle（真实 redis-cli；含持久化往返）

`scripts/test_redis.sh` 关键断言（除 CCR 已有的 PING/ECHO/SET/GET/PX/CONFIG/KEYS/INFO 外，新增）：

```bash
R="redis-cli -p 6379"
# 数据类型
$R RPUSH l a b c >/dev/null; test "$($R LLEN l)" = "3"
$R HSET h f1 v1 >/dev/null;   test "$($R HGET h f1)" = "v1"
$R ZADD z 1 a >/dev/null;     test "$($R ZSCORE z a)" = "1"
# 事务
test "$($R MULTI; $R SET tx 1; $R EXEC | tail -1)" != ""   # 视实现校验原子执行
# —— 交接探针：op_seq 持久化往返 ——
before=$($R WCOUNT)
$R SET k1 v1 >/dev/null; $R SET k2 v2 >/dev/null
mid=$($R WCOUNT); test "$mid" -gt "$before"
$R SAVE >/dev/null
# 重启服务器（kill + 重新拉起，载入 RDB）
after=$($R WCOUNT); test "$after" = "$mid"      # 重启后 op_seq 必须恢复一致
# 错误前缀不漂移
$R BADCMD 2>&1 | grep -q '^ERR '
echo "ALL STAGES + HANDOFF PROBE PASSED"
```

> 本机无 redis-cli/bash 时：参照 `codecrafters-redis.md` §7，可用裸 TCP RESP 探针等价复刻；或复用模型自筹的 `C:\Program Files\Redis\redis-cli.exe` + Git Bash。

---

## 5. cycle / 交接 判定（本案例真正要测的东西）

| 维度 | 通过 |
|------|------|
| **cycle 触发** | 节点流出现 **≥2 次 `cycle_advanced`**（降阈值后）；非 0 才算测到 |
| **干净换脑** | cycle 发生在 checklist 阶段**断点**（`pending_checkpoint` + 警告带），而非硬溢出急救（backlog C 的 `recover_context_overflow`） |
| **交接保真（核心）** | 跨 cycle 后 `WCOUNT` 持久化往返断言**仍绿**、错误前缀不漂移 → 开头的全局约束没在换脑中丢失 |
| **结构化状态延续** | cycle 后 checklist/plan 未回退、`in_progress_id` 连续，无重复已完成阶段 |
| **无早停** | 全程 `gate_skip ... open_items=0` 收尾，不得出现孤立 `incomplete_stop`；撞 step/context 应见 `step_limit_continue`/`context_warning` |
| **诚实性** | 全勾 ⇔ oracle exit 0（沿用 CCR §6 教训：先核引擎 todos 真值再信 UI） |

---

## 6. 离线回放与判定

```powershell
$log = "$env:USERPROFILE\.zagens\logs\sidecar.log"
Select-String -Path $log -Pattern 'cycle_advanced'            # 期望 ≥2 次
Select-String -Path $log -Pattern '\[lht-probe\]|verify_gate|gate_skip|context_warning'
Select-String -Path $log -Pattern 'carry_forward|<carry_forward>'   # 交接简报是否生成
```

导出线程后核：cycle 边界前后 checklist 快照连续性、`<carry_forward>` 简报是否如实带了「op_seq 全局约束 + 剩余阶段」，archive JSONL 是否落盘可检索。

**作为基线:** 记录墙钟（目标 ≥1H）、cycle 次数、每段 token、以及**交接探针是否一次过**，供后续 cycle 调优与并行实验参照。

---

**修订记录:**
- 2026-05-31 创建：REDIS-CYCLE 草案——CCR 加压续作，专测 cycle 触发（需先降 `[cycle.per_model]` 阈值）+ 跨 cycle 交接保真（op_seq/WCOUNT 持久化往返探针）。
