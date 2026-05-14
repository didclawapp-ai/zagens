# CRAFT 实施拆解：可落地的工作单元

> **关联文档**：[agent-reliability-craft-plan.md](agent-reliability-craft-plan.md) §11「后续改进方向」。
> **状态**：拆解中，逐项落地后打 ✅。
>
> **⚠️ 前置阅读**：Issue 0 是基础设施修正，必须在 Issue 1–6 之前完成。

---

## 优先级速览

| # | 标题 | 优先级 | 改动量 | 依赖 |
|---|------|--------|--------|------|
| 0 | 黑板路径修正：`current_dir()` → workspace | P0 | ~30 行 Rust | 无 |
| 1 | `GET /v1/blackboards/{task_id}` — 黑板 JSON API | P0 | ~30 行 Rust | #0 |
| 2 | `GET /v1/blackboards` — 列出所有 task | P0 | ~25 行 Rust | #0 |
| 3 | `structured_verdict` 注入 sentinel payload | P0 | ~12 行 Rust | 无 |
| 4 | `parse_structured_verdict` 追踪日志 | P1 | ~8 行 Rust | #3 |
| 5 | Task 状态卡片（DS Pick AgentPanel） | P1 | ~90 行 | #1 #2 |
| 6 | 指令文件自动发现（含 pick-rules 优先级） | P2 | ~20 行 Rust | #0 |
| 7 | A/B 验证 runbook | P1 | 文档 | 所有 P0 完成 |
| 8 | P2 fix-loop 手工验证 | P1 | 手工测试 | #3 |

---

## Issue 0（前置项）：黑板路径修正 —— `current_dir()` → workspace

**优先级**: P0 · **改动量**: ~30 行 Rust · **依赖**: 无

### 问题

`blackboard.rs:17-18` 用 `std::env::current_dir()` 做 blackboard 根目录：

```rust
fn workspace_root() -> Option<PathBuf> {
    std::env::current_dir().ok()
}
```

`write_blackboard_partition` 在 `run_subagent_task` 宿主任务中调用。DS Pick sidecar 的 spawn cwd 指向用户主目录时，黑板会落到 `~/`.deepseek/blackboards/`，而非 Composer / 线程工作区。`list_blackboard_tasks` 和 `read_blackboard_section`（子 Agent spawn 时调用）同样受影响。

**修正方向**：所有 blackboard 公开 API 以显式 `workspace: &Path` 参数替代隐式 `current_dir()`。`RuntimeApiState.workspace`（line 56）和 `task.runtime.context.workspace`（line 2650）已有工作区路径可用。

### 改动清单

**文件 1**：`crates/tui/src/tools/subagent/blackboard.rs`

```diff
- fn workspace_root() -> Option<PathBuf> {
-     std::env::current_dir().ok()
- }
-
- fn blackboard_path(task_id: &str) -> PathBuf {
-     let mut path = workspace_root()
-         .unwrap_or_else(|| PathBuf::from("."));
+ fn blackboard_path(workspace: &Path, task_id: &str) -> PathBuf {
+     let mut path = workspace.to_path_buf();
      path.push(".deepseek");
      path.push("blackboards");
      path.push(format!("{task_id}.json"));
      path
  }
```

所有公开函数签名加 `workspace: &Path` 首参数：

```diff
- pub fn read_blackboard_section(task_id: &str, agent_type: &SubAgentType) -> Option<String> {
+ pub fn read_blackboard_section(workspace: &Path, task_id: &str, agent_type: &SubAgentType) -> Option<String> {
-     let path = blackboard_path(task_id);
+     let path = blackboard_path(workspace, task_id);

- pub fn write_blackboard_partition(task_id: &str, agent_type: &SubAgentType, result: &SubAgentResult) {
+ pub fn write_blackboard_partition(workspace: &Path, task_id: &str, agent_type: &SubAgentType, result: &SubAgentResult) {
-     let path = blackboard_path(task_id);
+     let path = blackboard_path(workspace, task_id);

- pub fn read_blackboard_raw(task_id: &str) -> Option<serde_json::Value> {
+ pub fn read_blackboard_raw(workspace: &Path, task_id: &str) -> Option<serde_json::Value> {

- pub fn list_blackboard_tasks() -> Vec<String> {
+ pub fn list_blackboard_tasks(workspace: &Path) -> Vec<String> {
-     let root = workspace_root().unwrap_or_else(|| PathBuf::from("."))
-         .join(".deepseek").join("blackboards");
+     let root = workspace.join(".deepseek").join("blackboards");
```

**文件 2**：`crates/tui/src/tools/subagent/mod.rs` — 调用方更新

`run_subagent_task` 中（line 2677-2679），workspace 已在 `task.runtime.context.workspace`：

```diff
-         let _ = write_blackboard_partition(tid, &agent_type_for_blackboard, res);
+         let _ = write_blackboard_partition(&task.runtime.context.workspace, tid, &agent_type_for_blackboard, res);
```

`run_subagent` 中（line 2792-2795），workspace 来自 `runtime.context.workspace`：

```diff
-         .and_then(|tid| read_blackboard_section(tid, &agent_type));
+         .and_then(|tid| read_blackboard_section(&runtime.context.workspace, tid, &agent_type));
```

**文件 3**：`crates/tui/src/runtime_api.rs` — API handler 使用 `state.workspace`

（见 Issue 1、2 的 handler 代码）

**文件 4**：`crates/tui/src/tools/subagent/tests.rs` 和 `blackboard.rs` 内的 `#[cfg(test)]` 模块

函数签名变更后，两处测试均需适配：`blackboard_path("bugfix-001")` → `blackboard_path(&tmp_path, "bugfix-001")`。`blackboard.rs` 文件末尾有 `mod tests` 内联测试块（`test_blackboard_path_contains_task_id` 等），不可漏改。测试用 `std::env::temp_dir()` 构造 workspace。

### 改动量

| 文件 | 行数 |
|------|------|
| `subagent/blackboard.rs` | ~15（签名 + 实现） |
| `subagent/mod.rs` | ~4（调用方） |
| `runtime_api.rs` | 0（Issue 1/2 中已含） |
| `subagent/tests.rs` | ~8（测试适配） |
| **总计** | **~30** |

### 验收标准

```bash
# 启动 DS Pick，Composer 工作区设为 /projects/my-app
# 运行一次 CRAFT 流程后：
ls /projects/my-app/.deepseek/blackboards/
# → task-xxx.json（而不是 ~/）.deepseek/blackboards/
```

**`task_id` 安全约束**（同步写入 `write_blackboard_partition` 和 API handler）：

- `task_id` **禁止**包含 `/`、`\`、`..`（防路径越界）。
- 写入前用 `task_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')` 校验，非法字符返回 `Err` 或 400。

---

## Issue 1：`GET /v1/blackboards/{task_id}` — 黑板 JSON API

**优先级**: P0 · **改动量**: ~30 行 Rust · **依赖**: #0

### 验收标准

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:7878/v1/blackboards/task-20260515-001

# → 200 + JSON（从当前 workspace 的 .deepseek/blackboards/ 下读取）
# 不存在 → 404
# 无 token → 401
```

### 文件 1：`crates/tui/src/tools/subagent/blackboard.rs`

```rust
/// Read the full blackboard as a raw `serde_json::Value`.
/// Returns `None` when the file doesn't exist or is unparseable.
pub fn read_blackboard_raw(workspace: &Path, task_id: &str) -> Option<serde_json::Value> {
    let path = blackboard_path(workspace, task_id);
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}
```

### 文件 2：`crates/tui/src/runtime_api.rs`

路由（`"/v1/tasks/{id}/cancel"` 之后）：

```rust
.route("/v1/blackboards/{id}", get(get_blackboard))
```

Handler：

```rust
/// CRAFT: return full blackboard JSON for a task.
async fn get_blackboard(
    State(state): State<RuntimeApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err(ApiError::bad_request("task_id is required"));
    }
    crate::tools::subagent::blackboard::read_blackboard_raw(&state.workspace, task_id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("blackboard not found: {task_id}")))
}
```

### 改动量

| 文件 | 行数 |
|------|------|
| `subagent/blackboard.rs` | +8（函数）+ Issue 0 签名 |
| `runtime_api.rs` | +17 |
| **总计** | **~30**（含 Issue 0 分摊） |

---

## Issue 2：`GET /v1/blackboards` — 列出所有 task

**优先级**: P0 · **改动量**: ~25 行 Rust · **依赖**: #0

### 验收标准

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:7878/v1/blackboards
# → {"tasks": ["task-001", "task-002"]}
```

### 文件 1：`crates/tui/src/tools/subagent/blackboard.rs`

```rust
/// List all task_ids that have a blackboard file under the given workspace.
pub fn list_blackboard_tasks(workspace: &Path) -> Vec<String> {
    let root = workspace.join(".deepseek").join("blackboards");
    let dir = match std::fs::read_dir(&root) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    dir.filter_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        name.strip_suffix(".json").map(String::from)
    })
    .collect()
}
```

### 文件 2：`crates/tui/src/runtime_api.rs`

路由（`get_blackboard` 行之前）：

```rust
.route("/v1/blackboards", get(list_blackboards))
```

Handler：

```rust
#[derive(Serialize)]
struct BlackboardListResponse {
    tasks: Vec<String>,
}

async fn list_blackboards(
    State(state): State<RuntimeApiState>,
) -> Result<Json<BlackboardListResponse>, ApiError> {
    Ok(Json(BlackboardListResponse {
        tasks: crate::tools::subagent::blackboard::list_blackboard_tasks(&state.workspace),
    }))
}
```

### 改动量

| 文件 | 行数 |
|------|------|
| `subagent/blackboard.rs` | +10 |
| `runtime_api.rs` | +10 |
| **总计** | **~25**（含 Issue 0 分摊） |

---

## Issue 3：`structured_verdict` 注入 `<deepseek:subagent.done>` sentinel

**优先级**: P0 · **改动量**: ~12 行 Rust · **依赖**: 无

### 问题

当前 `subagent_done_sentinel`（`mod.rs:2755`）的 JSON payload 不含 `structured_verdict`。DS Pick 前端和主 Agent 需要额外调 `agent_result` 才能知道裁决。

原草案假定 `json!` 对 `Option::None` 自动省略键——**不是事实**。`serde_json::json!` 对 `None` 生成 `"structured_verdict": null`。若要"无则不出现"，必须手动组装 `serde_json::Value`。

### 验收标准

Reviewer 返回裁决时，sentinel JSON 中出现 `structured_verdict` 对象；Explore/Implementer 无裁决时，JSON 中该键**不存在**（非 `null`）。

```json
<deepseek:subagent.done>{"agent_id":"agent-xxx","agent_type":"review","status":"completed","duration_ms":1234,"steps":5,"summary":"...","structured_verdict":{"verdict":"BLOCKER","items":[...],"summary":"..."}}</deepseek:subagent.done>
```

### 文件：`crates/tui/src/tools/subagent/mod.rs`

替换 `subagent_done_sentinel` 中的 `json!` 为手动组装：

```rust
fn subagent_done_sentinel(agent_id: &str, res: &SubAgentResult) -> String {
    let mut payload = serde_json::Map::new();
    payload.insert("agent_id".into(), json!(agent_id));
    payload.insert("agent_type".into(), json!(res.agent_type.as_str()));
    payload.insert("status".into(), json!(subagent_status_name(&res.status)));
    payload.insert("duration_ms".into(), json!(res.duration_ms));
    payload.insert("steps".into(), json!(res.steps_taken));
    payload.insert("summary".into(), json!(summarize_subagent_result(res)));

    // CRAFT: include structured_verdict only when present.
    // Serialize failure → omit key (don't insert null as fallback).
    if let Some(ref v) = res.structured_verdict {
        if let Ok(val) = serde_json::to_value(v) {
            payload.insert("structured_verdict".into(), val);
        }
    }

    let payload = serde_json::Value::Object(payload);
    format!("<deepseek:subagent.done>{payload}</deepseek:subagent.done>")
}
```

### 改动量

| 文件 | 行数 |
|------|------|
| `subagent/mod.rs` | +12（替换原有 ~8 行） |
| **总计** | **~12** |

---

## Issue 4：`parse_structured_verdict` 追踪日志

**优先级**: P1 · **改动量**: ~8 行 Rust · **依赖**: #3

### 验收标准

`RUST_LOG=info` 启动后执行 CRAFT 流程，日志出现：

```
parse_structured_verdict: success (verdict=BLOCKER, items=2)
```

或：

```
parse_structured_verdict: no fence marker found, falling back to natural-language
```

### 文件：`crates/tui/src/tools/subagent/mod.rs`

在 `parse_structured_verdict` 函数（line 4189）中加 tracing：

```rust
fn parse_structured_verdict(text: &str) -> Option<StructuredVerdict> {
    let marker = "<!-- craft-verdict -->";
    let Some(after_marker) = text.find(marker).map(|idx| &text[idx + marker.len()..]) else {
        tracing::debug!("parse_structured_verdict: no fence marker found, falling back to natural-language");
        return None;
    };
    // ... existing JSON extraction to json_str ...
    match serde_json::from_str::<StructuredVerdict>(json_str) {
        Ok(v) => {
            tracing::info!(
                "parse_structured_verdict: success (verdict={}, items={})",
                serde_json::to_string(&v.verdict).unwrap_or_default(),
                v.items.len(),
            );
            Some(v)
        }
        Err(e) => {
            tracing::warn!("parse_structured_verdict: JSON parse failed: {e}");
            None
        }
    }
}
```

### 改动量

| 文件 | 行数 |
|------|------|
| `subagent/mod.rs` | ~8 |
| **总计** | **~8** |

---

## Issue 5：Task 状态卡片（DS Pick AgentPanel 底部）

**优先级**: P1 · **改动量**: ~90 行 · **依赖**: #1 #2

### 验收标准

DS Pick 右侧栏 AgentPanel 底部出现 "CRAFT Tasks" 区域，列出当前 workspace 下所有 blackboard task。每 5 秒自动刷新。

### 前置：字段对齐

本组件消费的 JSON 结构必须对齐 `agent-reliability-craft-plan.md` §5.3.1 的 blackboard schema。关键字段路径：

| UI 展示 | blackboard JSON 路径 | 类型 | 缺失时显示 |
|---------|---------------------|------|-----------|
| Explorer 完成 | `.explorer` 存在且非 null | bool | `—` |
| Implementer 轮数 | `.implementer.rounds` 数组长度 | number | `0` |
| Reviewer 裁决 | `.reviewer.verdict` | `"PASS"` / `"BLOCKER"` / `"MAJOR"` / `"FAIL"` | `—` |
| Verifier 摘要 | `.verifier.summary` | string | `—` |

> 注意：当前 `write_blackboard_partition` 中 Implementer 分区 `rounds` 为 placeholder（`json!([])`）。真实轮数需等黑板写入逻辑完善后才能正确展示。Issue 5 卡片目前展示的 Implementer 轮数是"已落盘的值"，不额外修补写入逻辑。
>
> **多工作区一致性**：`RuntimeApiState.workspace` 是 sidecar 的默认工作区，可能与 Composer / 恢复线程的实际工作区不一致。当前 Issue 5 的 `list_blackboard_tasks` 使用默认 workspace API——在多工作区并行场景下，卡片可能展示非当前 Composer 仓库的 task。后续可扩展 `?workspace=` 查询参数或由 Tauri command 传入 `workspaceRoot`，本稿不 blocking。

### Step 5a — Tauri command：`crates/desktop/src/commands.rs`

```rust
#[derive(Serialize)]
struct BlackboardTaskInfo {
    task_id: String,
    explorer_done: bool,
    implementer_rounds: usize,
    reviewer_verdict: Option<String>,
    verifier_summary: Option<String>,
}

#[tauri::command]
pub async fn list_blackboard_tasks(
    ctx: tauri::State<'_, AppContext>,
) -> Result<Vec<BlackboardTaskInfo>, String> {
    let base = format!("http://127.0.0.1:{}/v1/blackboards", ctx.runtime_port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let list_resp = client
        .get(&base)
        .header("Authorization", format!("Bearer {}", ctx.runtime_token))
        .send().await.map_err(|e| format!("list: {e}"))?;
    let list: serde_json::Value = list_resp.json().await.map_err(|e| format!("parse: {e}"))?;
    let tasks: Vec<String> = list["tasks"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut results = Vec::new();
    for task_id in tasks {
        let detail = client
            .get(format!("{base}/{task_id}"))
            .header("Authorization", format!("Bearer {}", ctx.runtime_token))
            .send().await.ok()
            .and_then(|r| r.json::<serde_json::Value>().ok());
        results.push(BlackboardTaskInfo {
            task_id: task_id.clone(),
            explorer_done: detail.as_ref().and_then(|d| d.get("explorer")).is_some(),
            implementer_rounds: detail.as_ref()
                .and_then(|d| d.get("implementer")?.get("rounds")?.as_array())
                .map(|a| a.len()).unwrap_or(0),
            reviewer_verdict: detail.as_ref()
                .and_then(|d| d.get("reviewer")?.get("verdict")?.as_str())
                .map(String::from),
            verifier_summary: detail.as_ref()
                .and_then(|d| d.get("verifier")?.get("summary")?.as_str())
                .map(String::from),
        });
    }
    Ok(results)
}
```

并在 `main.rs` 注册：`commands::list_blackboard_tasks,`

### Step 5b — API client：`crates/desktop/web-ui/src/api/client.ts`

```typescript
export interface BlackboardTask {
  task_id: string;
  explorer_done: boolean;
  implementer_rounds: number;
  reviewer_verdict: string | null;
  verifier_summary: string | null;
}

export async function fetchBlackboardTasks(): Promise<BlackboardTask[]> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<BlackboardTask[]>('list_blackboard_tasks');
}
```

### Step 5c — UI：`crates/desktop/web-ui/src/components/AgentPanel.tsx`

```tsx
import { fetchBlackboardTasks, type BlackboardTask } from '../api/client';

// 在 AgentPanel 已有 state 后加：
const [tasks, setTasks] = useState<BlackboardTask[]>([]);
useEffect(() => {
  if (!desktopHost) return;
  const poll = () => { fetchBlackboardTasks().then(setTasks).catch(() => {}); };
  poll();
  const iv = setInterval(poll, 5000);
  return () => clearInterval(iv);
}, [desktopHost]);
```

在 agent 列表之后渲染：

```tsx
{tasks.length > 0 && (
  <div className="mt-4 pt-3 border-t border-divider">
    <p className="text-[10px] font-semibold uppercase tracking-wider text-t-text-muted mb-2">
      CRAFT Tasks
    </p>
    {tasks.map((t) => (
      <div key={t.task_id} className="rounded-md border border-card-border bg-canvas-alt p-2 mb-1.5 text-xs">
        <div className="flex justify-between">
          <span className="font-mono text-[10px] text-t-text-muted truncate max-w-[120px]">{t.task_id}</span>
          <span className={
            t.reviewer_verdict === 'BLOCKER' ? 'text-t-error' :
            t.reviewer_verdict === 'PASS' ? 'text-success' : 'text-t-text-muted'
          }>{t.reviewer_verdict ?? '—'}</span>
        </div>
        <div className="flex gap-3 mt-1 text-[10px] text-t-text-muted">
          <span>E: {t.explorer_done ? '✓' : '—'}</span>
          <span>I: {t.implementer_rounds}r</span>
          <span>V: {t.verifier_summary ?? '—'}</span>
        </div>
      </div>
    ))}
  </div>
)}
```

### 改动量

| 文件 | 行数 |
|------|------|
| `commands.rs` | +50 |
| `main.rs` | +1 |
| `api/client.ts` | +12 |
| `AgentPanel.tsx` | +25 |
| **总计** | **~88** |

---

## Issue 6：指令文件自动发现（含 pick-rules 优先级）

**优先级**: P2 · **改动量**: ~20 行 Rust · **依赖**: #0（路径问题同源）

### 现状

当前仓库已有**两层**指令加载机制：

1. **`.deepseek/pick-rules.md`**（`prompts.rs:37`）：自动加载到 workspace 下的 `.deepseek/pick-rules.md`，通过 `merge_instruction_paths_with_pick_rules()` 在加载链中**排第一**，无需 `config.toml` 配置。
2. **`instructions = [...]`**（`config.rs:739`）：`config.toml` 显式路径列表，通过 `instructions_paths()` 解析。

两套机制已在 `merge_instruction_paths_with_pick_rules()` 中协调：pick-rules 优先，然后 config 路径（dedup by canonical path）。本 Issue 在此之上加**第三层**：自动扫描 `PROJECT_RULES.md` 和 `.cursor/rules/*.mdc`。

### 优先级约定（重要）

```
1. instructions = [...] 显式非空  → 只用显式列表（加 pick-rules 前缀）
2. instructions 未设置或空数组   → 自动发现 PROJECT_RULES.md + .cursor/rules/*.mdc（加 pick-rules 前缀）
```

`pick-rules.md` 在任何情况下都加载（只要文件存在）——它是 DS Pick 工作区规则编辑器的产物。本 Issue 只影响 config 路径的 fallback 逻辑。

### 验收标准

```bash
# 工作区有 PROJECT_RULES.md，config.toml 无 instructions
RUST_LOG=info cargo run -- serve --http
# 日志出现: "auto-discovered instruction: /workspace/PROJECT_RULES.md"

# 工作区有 .cursor/rules/security.mdc
# 日志出现: "auto-discovered instruction: /workspace/.cursor/rules/security.mdc"

# config.toml 有 instructions = ["custom.md"] → 不自动发现
```

### 文件：`crates/tui/src/config.rs`

`instructions_paths()` 方法（line 1425）的改动——用 `workspace` 参数替代 `std::env::current_dir()`：

```rust
/// Resolve instruction file paths.
///
/// Priority:
/// 1. Explicit `instructions = [...]` (non-empty) → use as-is.
/// 2. Otherwise → auto-discover PROJECT_RULES.md + .cursor/rules/*.mdc.
///
/// `pick-rules.md` is handled separately by `merge_instruction_paths_with_pick_rules`
/// and is NOT included here.
pub fn instructions_paths(&self, workspace: &Path) -> Vec<PathBuf> {
    // Explicit list takes priority.
    if let Some(explicit) = self.instructions.as_deref() {
        let non_empty: Vec<&str> = explicit.iter()
            .map(String::as_str).map(str::trim)
            .filter(|s| !s.is_empty()).collect();
        if !non_empty.is_empty() {
            return non_empty.into_iter().map(expand_path).collect();
        }
    }

    // Auto-discovery fallback.
    let mut paths: Vec<PathBuf> = Vec::new();

    let candidate = workspace.join("PROJECT_RULES.md");
    if candidate.is_file() {
        tracing::info!("auto-discovered instruction: {}", candidate.display());
        paths.push(candidate);
    }

    let cursor_rules = workspace.join(".cursor").join("rules");
    if let Ok(entries) = std::fs::read_dir(&cursor_rules) {
        // Collect then sort — stable order across machines preserves
        // prefix-cache hit rate when the same instructions are loaded
        // on different hosts.
        let mut mdc_files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "mdc"))
            .collect();
        mdc_files.sort();
        for p in mdc_files {
            tracing::info!("auto-discovered instruction: {}", p.display());
            paths.push(p);
        }
    }

    paths
}
```

**调用方更新**（`main.rs:4095`、`runtime_threads.rs:1986`、`tui/ui.rs:542`）：

```diff
- config.instructions_paths(),
+ config.instructions_paths(&workspace),
```

### 改动量

| 文件 | 行数 |
|------|------|
| `config.rs` | +15（替换） |
| `main.rs` + `runtime_threads.rs` + `tui/ui.rs` | +3（调用方 workspace 参数） |
| **总计** | **~18** |

---

## Issue 7：A/B 验证 runbook

**优先级**: P1 · **改动量**: 仅文档 · **依赖**: 所有 P0 issue 完成

### 目的

验证 CRAFT 角色链 + 黑板是否比单 Agent 长会话产生更高的首轮正确率和更少的 token 消耗。

### 实验设计

**任务池**：任选 3 个真实的 issue / 里程碑，覆盖难度梯度（简单、中等、跨模块）。以下为占位示例——执行时替换为当前仓库 open issue：

| # | 占位描述 | 难度 |
|---|---------|------|
| T1 | 修复单文件已知 bug（如某函数逻辑错误） | 简单 |
| T2 | 跨 2-3 个文件的功能增强（涉及 struct/serde 同步） | 中等 |
| T3 | 跨模块变更（如配置 → session 三层传递） | 跨模块 |

**对照组 A**：主 Agent 单次 prompt，允许自行 spawn 子 Agent（当前默认行为）。

**实验组 B**：主 Agent 遵循 CRAFT 链：`Explorer → Implementer → Reviewer → (Implementer) → Verifier`，所有子 Agent 带 `task_id`。

**每组各跑 3 次**（共 18 次实验），记录：

| 指标 | 记录方式 |
|------|----------|
| 首轮是否正确（无需人工修正） | Y/N |
| 闭环次数（Review → Revise 轮数） | 数 |
| `structured_verdict` 解析成功率 | `tracing` 日志 |
| Reviewer 假阳性（BLOCKER 但实际不需要修） | 人工判断 |
| 总 token 消耗 | `agent_result` 聚合 |
| 墙钟时间 | 手工计时 |

### 执行步骤

```bash
RUST_LOG=info cargo run --bin deepseek -- serve --http

# 对照组 A — T1（在新 session 中输入 T1 的 prompt）
# 实验组 B — T1（用 CRAFT 链，详见 craft-plan §6）
# 重复 A/B 各 3 次，覆盖 T1-T3
```

### 交付物

一份 Markdown 表格（18 行实验数据） + 结论段落（是否建议启用默认 CRAFT 链，或仅保留为可选模式）。

---

## Issue 8：P2 fix-loop 手工验证

**优先级**: P1 · **改动量**: 手工测试 · **依赖**: #3

### 目的

验证 P2 prompt（`base.md:285-308`）中的 fix-loop 协议是否真的被主 Agent 执行。当前闭环完全依赖 prompt 指令——没有 Rust 层强制兜底。如果主 Agent 跳过协议步骤，**这是 prompt 层的已知局限，不代表实现 bug**（计划 §5.4.1 已说明远期可视情况加 spawn 后钩子）。

### 验收标准

1. 构造 Reviewer **必定**返回 BLOCKER 的场景（如故意用 `thread_rng` 而非 `OsRng`）
2. 主 Agent 在同 turn 内调用 `agent_spawn(type="implementer", task_id="<same>")`，携带 blocker 的 `file`/`line`/`description`
3. 第二轮 Reviewer 返回 PASS → 主 Agent 继续下一步
4. 若 3 次闭环仍未 PASS → 主 Agent 升级用户并列出持久 blocker

### 验证项

- [ ] 主 Agent 在看到 `structured_verdict.verdict == "BLOCKER"` 后自动 `agent_spawn`
- [ ] 新 Implementer 的 prompt 包含 blocker 详情（`file` + `line` + `suggestion`）
- [ ] 同一 `task_id` 贯穿所有 spawn
- [ ] 3 次上限触发后升级用户（而非静默放弃或无限循环）

### 失败判定

若主 Agent 未执行闭环 → **记录为 prompt 指令跟随问题**（非 Rust 层 bug）。后续可由 Issue 追加减免（如 §5.4.1 选项 3 的 Rust 钩子）。

---

## 实施顺序建议

```
Issue 0 ──── 所有依赖 workspace 的前置修正
  ├─ Issue 1 ── Issue 2 ── Issue 5  (Task Dashboard 全链)
  ├─ Issue 6 ─────────────────────  (指令自动发现 — 同依赖 #0)
  └─ Issue 3 ── Issue 4 ──────────  (追踪日志 + sentinel)
                 └─ Issue 8 ──────  (fix-loop 验证 — 需 sentinel 到位)

Issue 7 ─────────────────────────── (A/B runbook — 需 1-4 完成)
```

第一周：Issue 0 + 1 + 2 + 3（~75 行 Rust），API 层 + sentinel 就绪。
第二周：Issue 4 + 5 + 6（~120 行），前端可见 + 日志 + 自动加载规则。
第三周：Issue 7 + 8（手工实验），驱动下一步决策。

---

**关联文档**: [agent-reliability-craft-plan.md](agent-reliability-craft-plan.md) · [SUBAGENTS.md](SUBAGENTS.md)
