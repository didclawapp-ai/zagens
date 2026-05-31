# MicroStack — Go 微服务框架（接口稳定性 + 重构抗性 + cycle 交接）

**案例编号:** MICROSTACK
**所属:** [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md)（长程压力 / cycle 加压 / 非破坏性修改）
**角色:** **最大规模 LHT 压测载体（目标 1.5–4 万行纯 Go）**，在 [`redis-cycle-handoff.md`](./redis-cycle-handoff.md) 的 cycle 三支柱之外，**新增两条现有案例（解释器 / Redis）都没覆盖的维度**：① **接口稳定性**（第一阶段冻结的 `contracts/` 接口零改动）；② **重构抗性**（换 Router 底层实现、外部 API/测试全绿）。
**与解释器 / CCR 的差异:** 解释器/Redis 是「单一可执行 + 单一 oracle」；MicroStack 是「**15+ 互相依赖的包 + 跨模块接口契约**」，逼出的是**长期架构稳定性**与**非破坏性修改**——这正是用户列的核心考验，前两类案例测不到。

> **⚠️ 这不是 cycle 首测案例。** cycle 触发 / 交接保真的**首测**仍以 [`redis-cycle-handoff.md`](./redis-cycle-handoff.md) 为准（更聚焦、探针更尖）。MicroStack 体量更大、但默认阈值下**同样未必触发 cycle**（见 §0）；它的不可替代价值是**接口稳定性 + 重构抗性**。要在本案例里测 cycle，走 §1 的「验证跑 A」（低阈值 + 单长 turn + 不手动回溯）。

---

## 0. ⚠️ 触发条件 + 开跑前校准（沿用 REDIS-CYCLE 的事实，不重复推导）

触发逻辑与「软护栏可能先截断」的校准方法**与 [`redis-cycle-handoff.md` §0](./redis-cycle-handoff.md) 完全一致**，此处只记差异：

- **默认阈值 768K 下，体量大 ≠ 会 cycle。** 大工具输出（`go test ./...` 整包日志、`go build` 全量输出）会被 large-output 路由截成摘要，活跃输入未必顶到 77%；软容量护栏（`capacity_flow/interventions.rs`）还可能在 768K 前 plateau。**结论：和 CCR 一样，整轮跑完可能仍无 cycle。** 这不是缺陷，是已知事实。
- **要测 cycle，靠「验证跑 A」（低阈值）**，不靠堆体量。配置同 REDIS-CYCLE：
```toml
# ~/.deepseek/config.toml — 验证跑专用，验证完删掉
[context.per_model.deepseek-v4-pro]
cycle_threshold = 120000     # trigger_floor = min(120K, 785K) = 120K，单个长 turn 内可达多次
```
- **阈值可配是本轮新修的前提**（`Config::cycle_runtime_config(model)` 真正把 `[context] cycle_threshold` / `[context.per_model.<model>]` 接进 `engine_spawn` 的 `CycleConfig`，默认 768K 不变）。详见 `CHANGELOG.md` 与 [`redis-cycle-handoff.md` §0](./redis-cycle-handoff.md)。
- 面板刷新节奏同 REDIS-CYCLE：长 turn 内每个 per-step 安全边界推 live 快照，仅单个长工具（cargo/go build 那几分钟）执行期不动属正常。

---

## 1. 两种跑法（**目标不同，prompt 不同，别混用**）

### A. cycle 验证跑（先做，几十分钟，专测换脑 + 自动交接）

**关键纪律——不要手动回溯。** 不能像原始设计那样每批让用户「复述 contracts/ 已有接口」：那是**人在替 harness 做交接**，会把 cycle 的 `carry_forward` 信号搞糊（模型后续一致，你分不清是交接保真还是吃了你刚贴的契约）。验证跑要**一次性长指令丢进去**，让它自然换脑后看还记不记得。

```
你是资深 Go 架构师，从零构建一个名为 MicroStack 的微服务框架（标准库优先，除数据库驱动外不引第三方 Web/ORM 框架）。串行完成、不要 spawn 子代理。这是一个长任务。

模块如何划分、实现顺序、文件/目录结构、checklist 怎么拆——全部由你自行规划。请先输出一份「架构契约清单」（各核心组件的接口与方法签名），冻结后再逐步实现。

【设计要求（框架须具备的能力，不规定实现方式）】
- 一个 HTTP 路由层：支持方法路由、路径参数、分组路由。
- 可组合的中间件链。
- 请求级 Context 抽象（参数绑定 / 取参 / JSON 响应 / 错误返回）。
- 配置加载（文件 + 环境变量 → 结构体）。
- 分级、结构化、可加字段的日志。
- 统一错误处理与错误码体系。
- 结构体校验（可注册自定义规则）。
- 应用生命周期管理（优雅启动 / 关闭 / 信号）。
- 一个基于本框架的示例 Todo 应用，跑通增删查改 HTTP 流程。

【贯穿全程的硬约束（从最早阶段就落地，保持到最后；这是项目的设计契约，不是阶段清单）】
- 接口优先 + 接口冻结：核心组件的接口集中放在 contracts/ 目录；一旦写出，后续绝不修改其中任何已声明的方法签名（只增不改）。
- 全局贯穿 X-Request-ID：入口中间件为每个请求生成/透传 X-Request-ID，它必须贯穿 Context → 每一条日志行 → 每一个错误响应体（末段任何模块产生的日志/错误都带得上同一个 request id）。
- 统一错误信封：所有对外错误经统一 Error 类型 + 错误码体系产生，错误响应体 JSON 恒含 {"code","message","request_id"}，绝不 panic / 不在请求路径上 unwrap。

【工程门禁（每完成一块就真实执行，不得把"写了代码"当"通过"）】
- go build ./... 必须过；go vet ./... 零告警、gofmt -d . 零差异；功能点对应的 go test ./... 全绿。
- 凡「编译 / 测试 / 跑示例」类验收项，用 [verify: <cmd>] 写进 checklist，并真实跑过。

【完成标准】
go build ./... 零错误、go vet ./... 零告警、gofmt -d . 零差异、go test ./... 全绿且覆盖率 ≥80%、示例 Todo 应用 HTTP 流程端到端跑通、contracts/ 自冻结后零改动。
```

目标：节点流出现 **≥2 次 `cycle_advanced`**；换脑后 §3 两条探针仍绿（contracts/ 未被改、request_id 末段仍贯穿）。

### B. 压力跑（验证 A 通过后，恢复默认 768K，**不改阈值**）

当作纯 LHT 长程压测 + **接口稳定性 + 重构抗性**测试，**不指望触发 cycle**。在 A 的核心层之上再追加下面的能力要求与终极对抗（同样：模块/顺序/拆解自定，约束不变）：

```
（在 A 的核心框架之上，继续扩展，沿用同一套 contracts/ 冻结 + X-Request-ID + 错误信封约束）
- 数据层：一个简易 ORM（结构体映射 / CRUD / 事务）先只做 MySQL 方言 + 一个可配最大连接数/空闲超时的连接池。
  —— ⚠️ Kafka/RabbitMQ/gRPC 服务发现/Redis 哨兵这类重集成能力，若不起真实服务就只能写编译桩 + 自写 mock 刷覆盖率，属"假绿高发区"（见 §4 注），默认不纳入验收；要做必须配真实集成测试。
- 工具层：一个能生成项目骨架的 CLI 脚手架 + 测试辅助包。

【终极对抗（所有模块完成后单独一批）】
在不改变 contracts/ 中任何已发布接口的前提下，把 HTTP Router 的内部实现重构为另一种数据结构（如前缀树 trie）；重构后所有中间件、示例 Todo 应用、以及全部已有 go test ./... 必须保持全绿。完成后给出 `git diff --stat contracts/`（应为空）。
```

---

## 2. 健康产物长这样（**模型自拆，下面只是参照**）

checklist 由模型自行拆解——**不要照搬下面的项**。一份健康的自拆 checklist 大致应覆盖到「设计要求」的各项能力（路由 / 中间件含 X-Request-ID / Context / 配置 / 日志 / 错误码 / 校验 / 生命周期 / Todo 示例），并以下面这组 `[verify:]` 验收项收口：

```
[verify: go build ./...]            全包编译通过
[verify: go vet ./...]              零告警
[verify: gofmt -d .]                零差异（输出为空）
[verify: go test ./... -cover]      全绿且总覆盖率 ≥80%
[verify: git diff --exit-code contracts/]   contracts/ 自冻结后零改动  ← 接口稳定性锚点
[verify: bash scripts/e2e_todo.sh]  示例 Todo 应用 HTTP 增删查改端到端跑通
```

> **能力覆盖**只作参照、用于判断这一跑覆盖面是否够（回归可比性），**不作为下发清单**。「创建了 contracts/ 文件」「写了 Todo 应用」**不算**通过——必须是上面命令真实 exit 0。沿用 DEMO3 铁律（[`../LHT_TEST_SUITE.md` §4](../LHT_TEST_SUITE.md)）。

---

## 3. 交接保真探针（cycle 专属 + 跨模块一致性，本案例核心）

两条**只在开头声明一次、要贯穿到最后**的横切约束，是故意设计的「跨 cycle 记忆探针」；任一在末段被破坏即说明换脑丢了开头状态：

| 探针 | 声明处 | 末段可判定的布尔 | 丢状态时的典型症状 |
|------|--------|------------------|--------------------|
| **接口稳定性** | 开头「contracts/ 冻结后不改」 | `git diff --exit-code contracts/` 退出 0 | cycle 后模型「重新设计」了某接口、加/改了方法签名 → diff 非空 |
| **X-Request-ID 贯穿** | 开头「贯穿 Context→日志→错误响应」 | e2e 脚本断言任一错误响应体含 `request_id`、且日志行带同一 id | 末段新模块产生的日志/错误漏掉 request_id（开头约束被摘掉） |

这比「整体能不能 build」更尖锐：它把**交接是否丢了开头的全局约束**变成两个可机器判定的布尔，对应 REDIS-CYCLE 的 `op_seq/WCOUNT` 探针。

---

## 4. 验收 oracle（全 exit-code，可离线回放）

| 验收项 | 命令 | 权重 |
|--------|------|------|
| 编译通过 | `go build ./...` | 必须项 |
| 代码规范 | `go vet ./...` 零告警 | 10% |
| 格式一致 | `gofmt -d .` 输出为空 | 5% |
| 单元测试 + 覆盖率 | `go test ./... -cover` 全绿且 ≥80% | 25% |
| **接口稳定性** | `git diff --exit-code contracts/`（冻结后） | 20% |
| **重构抗性** | Router 换 trie 后全部 `go test` 仍绿 + contracts/ diff 空 | 20% |
| 示例项目 | `bash scripts/e2e_todo.sh` 端到端跑通 | 20% |

> **⚠️ 假绿注（务必看，DEMO3 同类）：** `go build + 自写单测覆盖率 80%` 对**重集成模块（Kafka/RabbitMQ/gRPC/Redis 哨兵）证明力极弱**——模型既写实现又写测试，完全可以用「编译桩 + mock 刷到 80%」糊弄过去，**从没碰过真实中间件**。故本案例**默认把验收收窄到有干净 oracle 的核心层 + ORM(MySQL) + Todo 示例**；要测重集成模块，**必须起真实服务跑集成测试**，否则等于没测。覆盖率是自指指标，**接口稳定性（git diff）与重构抗性是本案例证明力最强的两条信号**，优先信它们。

`scripts/e2e_todo.sh` 关键断言：
```bash
BASE=http://127.0.0.1:8080
# 增删查改
id=$(curl -s -XPOST $BASE/todos -d '{"title":"t1"}' | jq -r .id)
curl -s $BASE/todos/$id | jq -e '.title=="t1"' >/dev/null
curl -s -XPUT $BASE/todos/$id -d '{"title":"t2"}' >/dev/null
curl -s $BASE/todos/$id | jq -e '.title=="t2"' >/dev/null
curl -s -XDELETE $BASE/todos/$id >/dev/null
# —— 交接探针：错误响应体必带 request_id ——
curl -si $BASE/todos/does-not-exist | grep -qi 'X-Request-Id'
curl -s  $BASE/todos/does-not-exist | jq -e '.code and .request_id' >/dev/null
echo "E2E + HANDOFF PROBE PASSED"
```

---

## 5. cycle / 交接 判定（验证跑 A 才测得到）

| 维度 | 通过 |
|------|------|
| **cycle 触发** | 节点流出现 **≥2 次 `cycle_advanced`**（降阈值后）；默认阈值下若 plateau < 75% 不触发，记录 plateau 值 + `GuardrailAction` 即为有效结论 |
| **干净换脑** | cycle 发生在 checklist 阶段**断点**（`pending_checkpoint` + 警告带），而非硬溢出急救 |
| **交接保真（核心）** | 跨 cycle 后 §3 两条探针仍绿：`git diff contracts/` 空 + 错误响应体仍带 request_id → 开头全局约束没在换脑中丢 |
| **结构化状态延续** | cycle 后 checklist/plan 未回退、`in_progress_id` 连续，无重复已完成阶段 |
| **无早停** | 全程 `gate_skip ... open_items=0` 收尾，无孤立 `incomplete_stop`；撞 step/context 见 `step_limit_continue`/`context_warning` |
| **诚实性** | 全勾 ⇔ oracle 全 exit 0（沿用 CCR §6 教训：先核引擎 todos 真值再信 UI） |

---

## 6. 离线回放与判定

```powershell
$log = "$env:USERPROFILE\.zagens\logs\sidecar.log"
Select-String -Path $log -Pattern 'cycle_advanced'                 # 验证跑 A 期望 ≥2 次
Select-String -Path $log -Pattern '\[lht-probe\]|verify_gate|gate_skip|context_warning'
Select-String -Path $log -Pattern 'carry_forward|<carry_forward>'  # 交接简报是否带「contracts/ 冻结 + request_id 贯穿」
```

导出线程后核：cycle 边界前后 checklist 快照连续性、`<carry_forward>` 简报是否如实带了**两条横切约束 + 剩余阶段**、archive JSONL 是否落盘可检索。

**作为基线:** 记录墙钟、cycle 次数（A）、每段 token、`git diff contracts/` 是否全程空、重构后测试是否一次全绿、e2e 探针是否一次过。

---

**修订记录:**
- 2026-05-31 创建：MICROSTACK 草案——Go 微服务框架最大规模 LHT。核心新维度为**接口稳定性（git diff contracts/）+ 重构抗性（Router 换 trie）**；cycle 测试走「验证跑 A（低阈值 + 单长 turn + 不手动回溯）」，明确**默认阈值下未必触发 cycle**、重集成模块（Kafka/gRPC/哨兵）为**假绿高发区**默认不纳入验收。cycle 首测仍以 REDIS-CYCLE 为准。
- 2026-05-31 改为**需求驱动 prompt**：删除原 §1 的阶段 A–I 顺序清单与 `contracts/*.go` 文件名等过度规定，模块划分/实现顺序/文件结构/checklist 拆解全交模型自定（更贴近真实、且不绕过 harness 从模型自产 plan/checklist 推任务图的被测能力）；只保留**项目设计要求 + 三条横切约束（contracts/ 冻结、X-Request-ID 贯穿、错误信封）+ `[verify:]` 门禁**——其中 contracts/ 目录约定与三条约束是 oracle/交接探针的依赖，必须留。§2 同步改为「自拆参照、非下发清单」。
