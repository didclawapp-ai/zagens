# DS Pick 系统设置面板 — 实施计划（修订版）

> **状态：** 草案（经代码评审修正）  
> **范围：** DS Pick 桌面端（`crates/desktop/`），Web UI + Tauri 后端  
> **目标：** 在右侧面板中增加完整的系统设置视图，将侧边栏底部的主题和语言选择器移入设置面板

---

## 0. 前置知识：ConfigToml ↔ Config 双轨 + Settings 第三轨

桌面端写入 `config.toml` 用的是 `crates/config/src/lib.rs` 的 **`ConfigToml`**（通过 `ConfigStore`），sidecar 运行时读取 `config.toml` 用的是 `crates/tui/src/config.rs` 的 **`Config`**。两个结构体**独立 serde 反序列化同一个 TOML 文件**，没有 `from_config_toml()` 桥接层。

这意味着新增任何字段，必须**同时在两个结构体中声明**，否则设置不生效：
- `ConfigToml` 缺失 → 桌面端写不入 TOML
- `Config` 缺失 → sidecar 重启后忽略该字段（serde 默认不拒绝未知字段，但也不会消费）

**⚠️ 注意 `cost_currency` 的特殊情况：** 该设置项的 TUI 消费路径不在 `Config` 而在 `Settings` 结构体（`crates/tui/src/settings.rs:L208`，持久化到 `settings.toml`，由 `/config` 运行时 API 读写）。这意味着仅往 `ConfigToml` + `Config` 双轨添加 `cost_currency` 字段**不足以让 sidecar 生效**——还需要在 TUI 启动流程中增加桥接逻辑：从 `Config.cost_currency` 读取并写入 `Settings.cost_currency`（详见 Step 1b 补充说明）。

---

## 1. 设置项清单（含现有状态标注）

对照 `crates/config/src/lib.rs`（**ConfigToml**）和 `crates/tui/src/config.rs`（**Config**）两个结构体的实际字段：

### 核心体验

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| 默认模型 | ✅ `default_text_model: Option<String>` | ✅ 已有 | `deepseek-v4-pro` | 下拉 |
| 推理深度 | ❌ 缺失 | ❌ 缺失（仅 `MessageRequest` 层有） | `max` | 分段按钮 |
| 货币单位 | ❌ 缺失 | ⚠️ 仅 `Settings.cost_currency`（`settings.toml`），不在 `Config` 中 | `usd` | 下拉 |

> ⚠️ `cost_currency` 的 TUI 消费路径：`Settings.cost_currency` → `/config` API → `app.cost_currency`（`commands/config.rs:L370`）。**不走 Config/config.toml 双轨**。需要在 Step 1b 增加桥接：TUI 启动时将 `Config.cost_currency` 同步到 `Settings.cost_currency`。
>
> 注：`reasoning_effort` 合法值为 `off` / `high` / `max` / `auto`（`auto` 由 `auto_reasoning.rs` 动态选择），已在 `client.rs:apply_reasoning_effort()` 中消费。

### 安全与行为

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| Shell 工具 | ❌ 缺失 | ✅ `allow_shell: Option<bool>` | `false`（桌面端） | 开关 |

> ⚠️ TUI 侧 `Config::allow_shell()` 默认 `unwrap_or(true)`，桌面端按 `false`。首次设置页未保存前，sidecar 可能仍以 shell 可用状态运行。保存后两边对齐。
| Web 搜索 | ❌ 缺失 | ✅ `features: Option<FeaturesToml>`（key `web_search`） | `true` | 开关 |
| 沙箱模式 | ✅ `sandbox_mode: Option<String>` | ✅ 已有 | `workspace-write` | 下拉 + ⚠️非 macOS 提示 |
| 审批策略 | ✅ `approval_policy: Option<String>` | ✅ 已有 | `on-request` | 下拉 |
| 执行策略 | ❌ 缺失 | ✅ `features.entries["exec_policy"]` | `true` | 开关 |
| 最大子代理数 | ❌ 缺失 | ✅ `max_subagents: Option<usize>` | `10` | 滑块 |

### 高级

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| LSP 诊断 | ✅ `lsp: Option<LspConfigToml>` | ✅ `lsp: Option<LspConfigToml>` | `true` | 开关 |
| 用户记忆 | ❌ 缺失 | ⚠️ `memory: Option<MemoryConfig>`（`enabled` 字段） | `false` | 开关 |
| 工作区快照 | ✅ `snapshots: Option<SnapshotsToml>` | ✅ 已有 | `true`（TUI 默认） | 开关 |
| 通知方式 | ❌ 缺失 | ⚠️ `notifications: Option<NotificationsConfig>`（`method` + `threshold_secs` + `include_summary`，3 字段） | `auto` | 下拉 |
| Session 文件上限 | ❌ 缺失 | ⚠️ 已有裸 TOML 解析（`session_manager.rs:L32-L53` 读 `[session] max_file_mb`），但 `Config` 结构体无此字段 | `5` MB | 数字输入 |

### 外观（从 Sidebar 移入）

| 设置项 | 存储位置 | 默认值 | UI 形式 |
|--------|----------|--------|---------|
| 主题 | `localStorage['deepseek-theme']` | OS 检测 | 切换按钮 |
| 界面语言 | `localStorage['ds-pick-locale']` | `zh-Hans`（自动检测） | 下拉 |

### 诊断信息（只读）

| 项目 | 来源 |
|------|------|
| 运行时连接状态 | `probeRuntimeConnection()` |
| 运行模式 | `desktopHost` prop（Tauri 或浏览器） |
| API Key 状态 | `get_api_key_status()` Tauri command |

---

## 2. 涉及文件（修订）

| # | 文件 | 改动类型 | 说明 |
|---|------|----------|------|
| 1 | `crates/config/src/lib.rs` | 修改 | `ConfigToml` 新增缺失字段 + 结构体 |
| 2 | **`crates/tui/src/config.rs`** | **修改** | **`Config` 新增匹配字段（关键：双轨同步）** |
| 3 | **`crates/tui/src/main.rs`** | **修改** | **`cost_currency` 桥接：从 `Config` 同步到 `Settings`（约 5 行）** |
| 4 | `crates/desktop/src/commands.rs` | 新增 | `get_system_settings` + `save_system_settings` |
| 5 | `crates/desktop/web-ui/src/components/SettingsPanel.tsx` | **新建** | 系统设置面板组件 |
| 6 | `crates/desktop/web-ui/src/components/RightPanel.tsx` | 修改 | 新增 `system` view，挂载 SettingsPanel；删除旧 settings 骨架 |
| 7 | `crates/desktop/web-ui/src/components/Sidebar.tsx` | 修改 | 移除主题/语言选择器；新增 `system` 子项 |
| 8 | `crates/desktop/web-ui/src/App.tsx` | 修改 | 将 `theme`/`onToggleTheme` 从 Sidebar 移到 RightPanel；新增 `get_platform_info` 调用 |
| 9 | `crates/desktop/web-ui/src/api/client.ts` | 新增 | `SystemSettings` 接口 + `fetchSystemSettings` / `saveSystemSettings` |
| 10 | `crates/desktop/web-ui/src/i18n/keys.ts` | 新增 | `settings` 命名空间翻译键 |
| 11 | `crates/desktop/web-ui/src/i18n/locales/zh-Hans.ts` | 新增 | 中文文案 |
| 12 | `crates/desktop/web-ui/src/i18n/locales/en.ts` | 新增 | 英文文案 |
| 13 | `crates/tui/src/session_manager.rs` | 修改 | `max_session_file_size()` 改为从 `Config` 读取（不再裸解析 TOML） |

> 相比原计划新增了文件 #2（`crates/tui/src/config.rs`）和 #3（`crates/tui/src/main.rs`），前者是双轨同步的核心，后者是 `cost_currency` 从 Config 到 Settings 的桥接。

---

## 3. 实施步骤

### 阶段 A：后端 — 两个 Config 结构体双轨同步

**Step 1 — `crates/config/src/lib.rs`：ConfigToml 新增缺失字段**

在现有 `ConfigToml` 中追加以下字段（标 ✅ 的已存在，无需添加）：

```rust
// 追加到 ConfigToml 结构体

/// 推理深度（off / high / max / auto）。V4 思维链控制。
#[serde(default)]
pub reasoning_effort: Option<String>,

/// 货币单位（usd / cny），用量仪表盘的数字解读货币。
#[serde(default)]
pub cost_currency: Option<String>,

/// Shell 工具开关。桌面端默认 false。
#[serde(default)]
pub allow_shell: Option<bool>,

/// 最大并发子代理数（1-20）。
#[serde(default)]
pub max_subagents: Option<usize>,

/// 功能开关。
#[serde(default)]
pub features: Option<FeaturesToml>,

/// 用户记忆。
#[serde(default)]
pub memory: Option<MemoryToml>,

/// 通知设置。
#[serde(default)]
pub notifications: Option<NotificationsToml>,

/// Session 文件上限（MB，0 = 不限制）。
#[serde(default)]
pub session: Option<SessionToml>,
```

新增结构体（放在 `ConfigToml` 下方）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeaturesToml {
    #[serde(default)]
    pub shell_tool: Option<bool>,
    #[serde(default)]
    pub subagents: Option<bool>,
    #[serde(default)]
    pub web_search: Option<bool>,
    #[serde(default)]
    pub apply_patch: Option<bool>,
    #[serde(default)]
    pub mcp: Option<bool>,
    #[serde(default)]
    pub exec_policy: Option<bool>,
    /// 兜底：捕获 `[features]` 表中不在上述 6 个命名字段中的任意 key，
    /// 避免 ConfigStore save 时静默丢弃用户手动添加的 feature 开关。
    #[serde(flatten)]
    pub extras: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryToml {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsToml {
    /// 通知方式：auto / osc9 / bel / off。默认 auto。
    #[serde(default)]
    pub method: Option<String>,
    /// 仅当对话轮次超过此秒数时通知。默认 30。
    #[serde(default)]
    pub threshold_secs: Option<u64>,
    /// 是否在通知正文中包含耗时和费用摘要。默认 false。
    #[serde(default)]
    pub include_summary: Option<bool>,
}
```

> **v1 范围说明：** 桌面系统设置面板 v1 仅暴露 `method` 字段。`threshold_secs` 和 `include_summary` 保留双轨声明（确保 save 时不丢失已有值），但 UI 暂不展示。后续版本可扩展。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToml {
    /// 大于此值的 session 文件拒绝加载（MB）。0 = 不限制。默认 5。
    #[serde(default = "default_session_max_file_mb")]
    pub max_file_mb: u64,
}

fn default_session_max_file_mb() -> u64 {
    5
}

impl Default for SessionToml {
    fn default() -> Self {
        Self {
            max_file_mb: default_session_max_file_mb(),
        }
    }
}
```

>
> **FeaturesToml 设计说明：** TUI 侧 `FeaturesToml`（`crates/tui/src/features.rs:L159`）使用 `BTreeMap<String, bool>` + `#[serde(flatten)]`，灵活但无编译期检查。桌面侧 `ConfigToml.FeaturesToml` 使用命名字段 + `#[serde(flatten)] extras` 兜底，兼顾类型安全与未知 key 兼容。两种设计的 TOML 输出格式完全一致：`[features]` 下的 `key = true/false`。`extras` 确保用户在 `config.toml` 中手动配置的其他 feature key（如未来新增的 feature）不会因系统设置保存而静默丢失。

**Step 1b — `crates/tui/src/config.rs`：Config 新增匹配字段**

在 TUI 侧 `Config` 结构体中追加（字段名必须与 TOML 键对齐）：

```rust
// 追加到 Config 结构体

/// 推理深度。未设置时默认 max。
#[serde(default)]
pub reasoning_effort: Option<String>,

/// 货币单位。未设置时默认 usd。
#[serde(default)]
pub cost_currency: Option<String>,

/// 用户记忆。
#[serde(default)]
pub memory: Option<MemoryConfig>,

/// 通知设置。
#[serde(default)]
pub notifications: Option<NotificationsConfig>,

/// Session 限制。
#[serde(default)]
pub session: Option<SessionConfig>,

// --- 配套结构体 ---

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NotificationsConfig {
    /// 通知方式：auto / osc9 / bel / off。默认 auto。
    #[serde(default)]
    pub method: NotificationMethod,
    /// 仅当对话轮次超过此秒数时通知。默认 30。
    #[serde(default = "default_threshold_secs")]
    pub threshold_secs: u64,
    /// 是否在通知正文中包含耗时和费用摘要。默认 false。
    #[serde(default)]
    pub include_summary: bool,
}

fn default_threshold_secs() -> u64 { 30 }

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_max_file_mb")]
    pub max_file_mb: u64,
}

fn default_session_max_file_mb() -> u64 { 5 }

impl Default for SessionConfig {
    fn default() -> Self {
        Self { max_file_mb: default_session_max_file_mb() }
    }
}

impl Config {
    /// 读取 session 文件上限：`[session] max_file_mb` > 环境变量 > 默认 5。
    /// **0 表示不限制**（返回 `u64::MAX`），与现有 `max_session_file_size()` 语义一致。
    #[must_use]
    pub fn session_max_file_mb(&self) -> u64 {
        // 1. TOML [session] max_file_mb
        if let Some(cfg) = self.session.as_ref() {
            return if cfg.max_file_mb > 0 {
                cfg.max_file_mb
            } else {
                u64::MAX // 0 = 不限制
            };
        }
        // 2. 环境变量（向后兼容）
        if let Ok(val) = std::env::var("DEEPSEEK_MAX_SESSION_FILE_MB") {
            if let Ok(mb) = val.trim().parse::<u64>() {
                return if mb > 0 { mb } else { u64::MAX };
            }
        }
        // 3. 默认 5 MB
        5
    }
}
```

> **⚠️ `cost_currency` 桥接补充：** 由于 TUI 侧 `cost_currency` 的实际消费路径是 `Settings.cost_currency`（`settings.toml`）而非 `Config.cost_currency`（`config.toml`），需要在 TUI 启动流程中增加桥接逻辑。改动点（`crates/tui/src/main.rs`）：
>
> 在 sidecar 启动后、`app.cost_currency` 初始化前，插入：
> ```rust
> // 桥接 config.toml 的 cost_currency 到 Settings（桌面系统设置）
> if let Some(ref cc) = config.cost_currency {
>     let parsed = crate::pricing::CostCurrency::from_setting(cc)
>         .unwrap_or(crate::pricing::CostCurrency::Usd);
>     app.cost_currency = parsed;
> }
> ```
> 该逻辑应放在现有 `Settings::load()` 之后，使 config.toml 的值覆盖 settings.toml 的默认值。同时需在 `commands/config.rs` 的 `cost_currency` setter 中保持单向同步（运行时修改仍写入 settings.toml，不受影响）。

同时在 `session_manager.rs` 中将 `max_session_file_size()` 改为接受 `Config` 引用：

当前 `max_session_file_size()` 是顶层独立函数（line 32），只读环境变量。需改为 `SessionManager::new()` 或 `SessionManager::load()` 时从 `Config::session_max_file_mb()` 取值为字段存储，`max_session_file_size()` 改为读该字段。改动点：
- `SessionManager` 新增字段 `max_session_file_bytes: u64`
- 构造时从 `config.session_max_file_mb() * 1024 * 1024` 计算
- `max_session_file_size()` 改为方法或直接从字段读取

---

**Step 2 — `crates/desktop/src/commands.rs` 新增 Tauri commands**

> 与 Step 1 同步：现在 `reasoning_effort`、`allow_shell`、`max_subagents`、`features`、`memory`、`notifications`、`session` 在 `ConfigToml` 中均已存在。

```rust
use deepseek_config::ConfigStore;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemSettings {
    pub default_model: String,
    pub reasoning_effort: String,
    pub cost_currency: String,
    pub allow_shell: bool,
    pub approval_policy: String,
    pub sandbox_mode: String,
    pub max_subagents: usize,
    pub web_search: bool,
    pub exec_policy: bool,
    pub memory_enabled: bool,
    pub lsp_enabled: bool,
    pub snapshots_enabled: bool,
    pub notify_method: String,
    pub session_file_mb: u64,
}

#[tauri::command]
pub fn get_system_settings() -> Result<SystemSettings, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &store.config;
    Ok(SystemSettings {
        default_model: cfg.default_text_model.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "deepseek-v4-pro".into()),
        reasoning_effort: cfg.reasoning_effort.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "max".into()),
        cost_currency: cfg.cost_currency.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "usd".into()),
        allow_shell: cfg.allow_shell.unwrap_or(false),
        approval_policy: cfg.approval_policy.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "on-request".into()),
        sandbox_mode: cfg.sandbox_mode.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "workspace-write".into()),
        max_subagents: cfg.max_subagents.unwrap_or(10).clamp(1, 20),
        web_search: cfg.features.as_ref()
            .and_then(|f| f.web_search).unwrap_or(true),
        exec_policy: cfg.features.as_ref()
            .and_then(|f| f.exec_policy).unwrap_or(true),
        memory_enabled: cfg.memory.as_ref()
            .and_then(|m| m.enabled).unwrap_or(false),
        lsp_enabled: cfg.lsp.as_ref()
            .and_then(|l| l.enabled).unwrap_or(true),
        snapshots_enabled: cfg.snapshots.as_ref()
            .map(|s| s.enabled)
            .unwrap_or(true),  // SnapshotsToml.enabled 是 bool（非 Option）
        notify_method: cfg.notifications.as_ref()
            .and_then(|n| n.method.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "auto".into()),
        session_file_mb: cfg.session.as_ref()
            .map(|s| s.max_file_mb)
            .unwrap_or(5),
    })
}

#[tauri::command]
pub fn save_system_settings(
    settings: SystemSettings,
    ctx: tauri::State<'_, AppContext>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &mut store.config;

    // 顶层标量字段
    cfg.default_text_model = Some(settings.default_model);
    cfg.reasoning_effort = Some(settings.reasoning_effort);
    cfg.cost_currency = Some(settings.cost_currency);
    cfg.allow_shell = Some(settings.allow_shell);
    cfg.approval_policy = Some(settings.approval_policy);
    cfg.sandbox_mode = Some(settings.sandbox_mode);
    cfg.max_subagents = Some(settings.max_subagents);

    // features：使用 get_or_insert_with 而非 take() ——
    // 避免丢弃 config.toml 中已有的其他 features 字段
    let features = cfg.features.get_or_insert_with(Default::default);
    features.web_search = Some(settings.web_search);
    features.exec_policy = Some(settings.exec_policy);

    // memory
    let memory = cfg.memory.get_or_insert_with(Default::default);
    memory.enabled = Some(settings.memory_enabled);

    // lsp（ConfigToml 中 LspConfigToml 已存在）
    let lsp = cfg.lsp.get_or_insert_with(Default::default);
    lsp.enabled = Some(settings.lsp_enabled);

    // snapshots
    let snapshots = cfg.snapshots.get_or_insert_with(Default::default);
    snapshots.enabled = settings.snapshots_enabled;

    // notifications
    let notif = cfg.notifications.get_or_insert_with(Default::default);
    notif.method = Some(settings.notify_method);

    // session
    let session = cfg.session.get_or_insert_with(Default::default);
    session.max_file_mb = settings.session_file_mb;

    store.save().map_err(|e| e.to_string())?;

    // 重启 sidecar 使 TUI Config 重新读取 config.toml
    ctx.sidecar_restart.notify_one();
    Ok(())
}
```

**前端 `api/client.ts` 新增调用：**

```typescript
export interface SystemSettings {
  default_model: string;
  reasoning_effort: string;
  cost_currency: string;
  allow_shell: boolean;
  approval_policy: string;
  sandbox_mode: string;
  max_subagents: number;
  web_search: boolean;
  exec_policy: boolean;
  memory_enabled: boolean;
  lsp_enabled: boolean;
  snapshots_enabled: boolean;
  notify_method: string;
  session_file_mb: number;
}

export async function fetchSystemSettings(): Promise<SystemSettings> {
  return invoke<SystemSettings>('get_system_settings');
}

export async function saveSystemSettings(settings: SystemSettings): Promise<void> {
  await invoke('save_system_settings', { settings });
}
```

---

### 阶段 B：前端 — 组件重构

**Step 3 — `Sidebar.tsx` 移除底部主题 + 语言选择器**

- 删除 `<button onClick={onToggleTheme}>` 主题切换按钮
- 删除 `<select value={locale}>` 语言选择器
- Props 接口删除 `theme: Theme; onToggleTheme: () => void`
- 保留版本号 `DS Pick v0.2.2` 和运行时状态指示器

**Step 4 — `Sidebar.tsx` SettingsAccordion 新增 `system` 子项**

```typescript
type SettingsTab =
  | 'api-key' | 'mcp' | 'usage' | 'tasks-skills'
  | 'agents' | 'routing' | 'system';

// subItems 尾部追加
{ tab: 'system', label: '系统设置', show: true },
```

**Step 5 — 新建 `SettingsPanel.tsx`**

组件结构与原计划一致。新增 prop `platform: string`（从 `get_platform_info()` 的 `os` 字段传入，如 `"darwin"` / `"linux"` / `"windows"`）。在沙箱模式下拉框增加平台判断：

```tsx
{/* 非 macOS 平台沙箱当前为纯透传（参见 sandbox/mod.rs），
    在 UI 中保留选项但附加说明文字 */}
{platform !== 'darwin' && (
  <p className="text-[11px] text-t-text-muted mt-0.5">
    {t('settings.sandboxNotEnforced')}
  </p>
)}
```

对应 i18n 键：
- `zh-Hans`: `"当前平台沙箱隔离尚未完全生效；此选项控制策略声明，实际执行依赖后续版本。"`
- `en`: `"Sandbox isolation is not yet enforced on this platform; this setting controls policy declaration only."`

**Step 6 — `RightPanel.tsx` 挂载 SettingsPanel**

> ⚠️ **关键：删除旧的 `view === 'settings'` 内联内容。** 当前 `RightPanel.tsx` L703-L736 有一段内联渲染的 settings skeleton（主题切换按钮 + 运行时状态 + Tauri 检测文本，约 30 行）。这些内容由 `SettingsPanel` 完全替代，必须整体移除，否则会出现重复渲染。

```typescript
export type RightPanelView =
  | 'workspace' | 'api-key' | 'settings' | 'system'
  | 'mcp' | 'usage' | 'tasks-skills' | 'agents' | 'routing';

// Props 新增
interface Props {
  // ... 现有 ...
  theme: Theme;
  onToggleTheme: () => void;
  platform: string;     // "darwin" / "linux" / "windows"
}
```

渲染分支（**替换**原有的 `view === 'settings'` 分支，删除旧骨架）：
```tsx
{(view === 'settings' || view === 'system') && (
  <SettingsPanel
    runtimeConn={runtimeConn}
    desktopHost={desktopHost}
    apiKeyConfigured={apiKeyConfigured}
    platform={platform}
    theme={theme}
    onToggleTheme={onToggleTheme}
  />
)}
```

**Step 7 — `App.tsx` 适配**

- 从 `<Sidebar>` 移除 `theme` / `onToggleTheme` props，改为传入 `<RightPanel>`
- **新增 `get_platform_info()` 调用：** 当前 `App.tsx` 中并无此调用，需要新增。在 `refreshApiKeyStatus` 的 Tauri 检测逻辑旁边追加：

```typescript
// App.tsx 新增 state
const [platform, setPlatform] = useState<string>('unknown');

// 在 detectDesktopHost / refreshApiKeyStatus 的 invoke 检测块中追加
const info = await invoke<{ os: string; arch: string; version: string }>('get_platform_info');
setPlatform(info.os); // "windows" | "darwin" | "linux"
```

然后将 `platform` 作为 prop 传入 `<RightPanel>`，由 RightPanel 透传给 `<SettingsPanel>` 用于沙箱模式的平台判断。

---

### 阶段 C：国际化

**Step 8 — `i18n/keys.ts` 新增 `settings` 命名空间**

```typescript
settings: {
  _section: '';  core: '';  security: '';  advanced: '';  appearance: '';
  defaultModel: '';  reasoningEffort: '';  reasoningOff: '';  reasoningHigh: '';
  reasoningMax: '';  reasoningAuto: '';
  costCurrency: '';  shellTool: '';  webSearch: '';
  sandboxMode: '';  sandboxReadOnly: '';  sandboxWorkspace: '';
  sandboxFullAccess: '';  sandboxNotEnforced: '';
  approvalPolicy: '';  approvalOnRequest: '';  approvalUntrusted: '';  approvalNever: '';
  execPolicy: '';  maxSubagents: '';
  lspDiag: '';  lspDiagDesc: '';  userMemory: '';  userMemoryDesc: '';
  snapshots: '';  snapshotsDesc: '';
  notifyMethod: '';  notifyAuto: '';  notifyOsc9: '';  notifyBel: '';  notifyOff: '';
  sessionFileLimit: '';  sessionFileLimitDesc: '';
  theme: '';  themeLight: '';  themeDark: '';  language: '';
  runtimeStatus: '';  runtimeConn: '';  desktopMode: '';  browserMode: '';
  configured: '';  notConfigured: '';  save: '';  saving: '';
}
```

**Step 9 — `zh-Hans.ts` / `en.ts` 中文案**

与原计划一致，新增以下额外键：

| 键 | 中文 | English |
|----|------|---------|
| `reasoningAuto` | 自动 | Auto |
| `sandboxNotEnforced` | 当前平台沙箱隔离尚未完全生效；此选项控制策略声明，实际执行依赖后续版本。 | Sandbox isolation is not yet enforced on this platform; this setting controls policy declaration only. |

---

## 4. 修订版改动量

| 文件 | 新增行 | 修改行 | 删除行 | 净增 |
|------|--------|--------|--------|------|
| `crates/config/src/lib.rs` | ~70 | — | — | +70 |
| `crates/tui/src/config.rs` | ~50 | ~5 | — | +55 |
| `crates/tui/src/main.rs` | ~10 | — | — | +10 |
| `commands.rs` | ~130 | — | — | +130 |
| `SettingsPanel.tsx` | ~210 | — | — | +210 |
| `RightPanel.tsx` | ~20 | ~10 | ~35 | -5 |
| `Sidebar.tsx` | ~5 | ~10 | ~40 | -25 |
| `App.tsx` | ~15 | ~5 | ~5 | +10 |
| `client.ts` | ~30 | — | — | +30 |
| `i18n/keys.ts` | ~42 | — | — | +42 |
| `i18n/zh-Hans.ts` | ~38 | — | — | +38 |
| `i18n/en.ts` | ~38 | — | — | +38 |
| `session_manager.rs` | — | ~3 | ~3 | +0 |
| **合计** | **~660** | **~33** | **~83** | **~605** |

> 比原计划多约 135 行，额外增量来自：
> - TUI `Config` 双轨同步字段 + `SessionConfig` + `NotificationsConfig` 补全字段（~55 行）
> - `cost_currency` Config→Settings 桥接逻辑（~10 行）
> - `App.tsx` 新增 `get_platform_info` 调用（~10 行）
> - `client.ts` TypeScript 接口定义（~30 行）
> - `NotificationsToml` 从 1 字段扩为 3 字段（~10 行，config crate 侧）

---

## 5. 与原计划的关键差异

| 问题 | 原计划 | 修订版 |
|------|--------|--------|
| ConfigToml ↔ Config 双轨 | 未提及 | 新增 `crates/tui/src/config.rs` 修改（Step 1b） |
| 字段状态标注 | 全部笼统标为"新增" | 精确标注 ✅/❌/⚠️ 三种状态 |
| `save_system_settings` 写嵌套表 | `take().unwrap_or_default()` → 丢弃现有字段 | `get_or_insert_with(Default::default)` → 保留现有字段 |
| `max_subagents` 型问题 | 字段不存在但代码中使用 | 两处均新增声明 |
| Session 文件上限 | 仅设环境变量 → 重启丢失 | 新增 `[session] max_file_mb` TOML 表项 → 持久化 |
| 沙箱模式 UX | 无平台区分 | 非 macOS 附加说明文字 |
| `reasoning_effort` 枚举值 | 仅 `off/high/max` | 补充 `auto`（由 `auto_reasoning.rs` 消费） |
| **`cost_currency` 存储路径** | 未说明（假定走 Config/config.toml 双轨） | **发现不走双轨：TUI 消费路径为 `Settings.cost_currency`（`settings.toml`）**，需在 `main.rs` 增加 Config→Settings 桥接 |
| **`get_platform_info` 调用** | 声称 App.tsx 已调用 | **实际未调用**，需新增 state + invoke |
| **`session_max_file_mb` 0=无限语义** | 未考虑 | 补充 `mb == 0 → u64::MAX` 逻辑（对齐现有 `max_session_file_size()` 行为） |
| **NotificationsConfig 字段完整性** | 仅声明 `method` | 补全为 3 字段（`method` + `threshold_secs` + `include_summary`），v1 UI 仅暴露 `method` |
| **旧 settings 视图替换** | 未明确 | 明确标注需删除 `RightPanel.tsx` L703-L736 的内联 settings skeleton |
| **Sidecar 重启错误处理** | 未提及 | 标注为 v1 已知限制（fire-and-forget） |

---

## 6. 已知限制与风险（v1）

| 限制 | 影响 | 缓解措施 |
|------|------|----------|
| `save_system_settings` 后 sidecar 重启是 fire-and-forget（`notify_one()` 不等待结果） | 若 config.toml 写入成功但 TUI 启动失败，用户只能手动感知（runtime 连接变为 `offline`） | v1 不做处理；后续版本可增加 sidecar health check 轮询并向前端报告启动失败原因 |
| `cost_currency` 变更后 sidecar 重启，`Settings.cost_currency` 被 config.toml 的值覆盖 | 用户若在 TUI 内部通过 `/config` 修改了货币单位，桌面端保存后会被覆盖（config.toml 优先级高于运行时） | 这是预期行为：桌面端系统设置为权威来源。TUI 内的运行时修改不持久化到 settings.toml 时才能避免冲突 |
| `save_system_settings` 写入 `features` 嵌套表时，仅触碰 `web_search` 和 `exec_policy` 两个 key | **数据丢失风险：** `ConfigToml.FeaturesToml` 使用命名字段（无 `#[serde(flatten)]`），若用户 `config.toml` 的 `[features]` 表中有其他 key（如 `apply_patch`, `mcp`, `subagents` 等），通过 ConfigStore 保存时这些未知 key 会被 serde **静默丢弃**（serde 默认忽略未知字段，序列化时仅输出 6 个命名字段，不同于 TUI 的 `FeaturesToml` 使用 `BTreeMap` 可保留所有 key） | **修复方案：** 给 `ConfigToml.FeaturesToml` 增加 `#[serde(flatten)] extras: BTreeMap<String, toml::Value>` 兜底字段，确保未知 key 在反序列化→序列化往返中不丢失。或者在 `save_system_settings` 的 features 写入处，先读取整个 `[features]` 表为 `toml::Value`，合并后再写回 |
| 桌面端首次保存前，`allow_shell` 的 TUI 默认值为 `true`，桌面端按 `false` | 首次打开设置页时，sidecar 中 shell 可能仍是可用状态 | 保存后立即重启 sidecar 对齐；建议在 UI 中标注"保存后生效" |
