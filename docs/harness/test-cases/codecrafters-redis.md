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

**修订记录:**
- 2026-05-30 创建:CodeCrafters Redis 单模型串行规格(可复制 prompt + `[verify:]` + redis-cli oracle + 串行基线测量)。
