# DS Pick 系统设置面板 — 实施计划（修订版）

> **状态：** 草案（经代码评审修正）  
> **范围：** DS Pick 桌面端（`crates/desktop/`），Web UI + Tauri 后端  
> **目标：** 在右侧面板中增加完整的系统设置视图，将侧边栏底部的主题和语言选择器移入设置面板

---

## 0. 前置知识：ConfigToml ↔ Config 双轨

桌面端写入 `config.toml` 用的是 `crates/config/src/lib.rs` 的 **`ConfigToml`**（通过 `ConfigStore`），sidecar 运行时读取 `config.toml` 用的是 `crates/tui/src/config.rs` 的 **`Config`**。两个结构体**独立 serde 反序列化同一个 TOML 文件**，没有 `from_config_toml()` 桥接层。

这意味着新增任何字段，必须**同时在两个结构体中声明**，否则设置不生效：
- `ConfigToml` 缺失 → 桌面端写不入 TOML
- `Config` 缺失 → sidecar 重启后忽略该字段（serde 默认不拒绝未知字段，但也不会消费）

---

## 1. 设置项清单（含现有状态标注）

对照 `crates/config/src/lib.rs`（**ConfigToml**）和 `crates/tui/src/config.rs`（**Config**）两个结构体的实际字段：

### 核心体验

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| 默认模型 | ✅ `default_text_model: Option<String>` | ✅ 已有 | `deepseek-v4-pro` | 下拉 |
| 推理深度 | ❌ 缺失 | ❌ 缺失（仅 `MessageRequest` 层有） | `max` | 分段按钮 |
| 货币单位 | ❌ 缺失 | ⚠️ 仅 `Settings.cost_currency` 字符串 | `usd` | 下拉 |

注：`reasoning_effort` 合法值为 `off` / `high` / `max` / `auto`（`auto` 由 `auto_reasoning.rs` 动态选择），已在 `client.rs:apply_reasoning_effort()` 中消费。

### 安全与行为

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| Shell 工具 | ❌ 缺失 | ✅ `allow_shell: Option<bool>` | `false`（桌面端） | 开关 |
| Web 搜索 | ❌ 缺失 | ✅ `features: Features`（含 `web_search`） | `true` | 开关 |
| 沙箱模式 | ✅ `sandbox_mode: Option<String>` | ✅ 已有 | `workspace-write` | 下拉 + ⚠️非 macOS 提示 |
| 审批策略 | ✅ `approval_policy: Option<String>` | ✅ 已有 | `on-request` | 下拉 |
| 执行策略 | ❌ 缺失 | ✅ `features.exec_policy` | `true` | 开关 |
| 最大子代理数 | ❌ 缺失 | ✅ `max_subagents: Option<usize>` | `10` | 滑块 |

### 高级

| 设置项 | ConfigToml 状态 | Config 状态 | 默认值 | UI 形式 |
|--------|----------------|-------------|--------|---------|
| LSP 诊断 | ✅ `lsp: Option<LspConfigToml>` | ✅ `lsp: Option<LspConfigToml>` | `true` | 开关 |
| 用户记忆 | ❌ 缺失 | ❌ 缺失 | `false` | 开关 |
| 工作区快照 | ✅ `snapshots: Option<SnapshotsToml>` | ✅ 已有 | `true`（TUI 默认） | 开关 |
| 通知方式 | ❌ 缺失 | ❌ 缺失 | `auto` | 下拉 |
| Session 文件上限 | ❌ 缺失 | ❌ 缺失（仅读 `env::var`） | `5` MB | 数字输入 |

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
| 3 | `crates/desktop/src/commands.rs` | 新增 | `get_system_settings` + `save_system_settings` |
| 4 | `crates/desktop/web-ui/src/components/SettingsPanel.tsx` | **新建** | 系统设置面板组件 |
| 5 | `crates/desktop/web-ui/src/components/RightPanel.tsx` | 修改 | 新增 `system` view，挂载 SettingsPanel |
| 6 | `crates/desktop/web-ui/src/components/Sidebar.tsx` | 修改 | 移除主题/语言选择器；新增 `system` 子项 |
| 7 | `crates/desktop/web-ui/src/App.tsx` | 修改 | 将 `theme`/`onToggleTheme` 从 Sidebar 移到 RightPanel |
| 8 | `crates/desktop/web-ui/src/i18n/keys.ts` | 新增 | `settings` 命名空间翻译键 |
| 9 | `crates/desktop/web-ui/src/i18n/locales/zh-Hans.ts` | 新增 | 中文文案 |
| 10 | `crates/desktop/web-ui/src/i18n/locales/en.ts` | 新增 | 英文文案 |

> 相比原计划新增了文件 #2（`crates/tui/src/config.rs`），这是双轨同步的核心。

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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryToml {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsToml {
    #[serde(default)]
    pub method: Option<String>,
}

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
    #[serde(default)]
    pub method: Option<String>,
}

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
    #[must_use]
    pub fn session_max_file_mb(&self) -> u64 {
        if let Some(cfg) = self.session.as_ref()
            && cfg.max_file_mb > 0
        {
            return cfg.max_file_mb;
        }
        std::env::var("DEEPSEEK_MAX_SESSION_FILE_MB")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(5)
    }
}
```

同时在 `session_manager.rs` 中将 `env::var("DEEPSEEK_MAX_SESSION_FILE_MB")` 替换为 `config.session_max_file_mb()`。

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

组件结构与原计划一致。在沙箱模式下拉框增加平台判断注释：

```tsx
{/* 非 macOS 平台沙箱当前为纯透传（参见 sandbox/mod.rs），
    在 UI 中保留选项但附加说明文字 */}
{desktopHost && platform !== 'darwin' && (
  <p className="text-[11px] text-t-text-muted mt-0.5">
    {t('settings.sandboxNotEnforced')}
  </p>
)}
```

对应 i18n 键：
- `zh-Hans`: `"当前平台沙箱隔离尚未完全生效；此选项控制策略声明，实际执行依赖后续版本。"`
- `en`: `"Sandbox isolation is not yet enforced on this platform; this setting controls policy declaration only."`

**Step 6 — `RightPanel.tsx` 挂载 SettingsPanel**

```typescript
export type RightPanelView =
  | 'workspace' | 'api-key' | 'settings' | 'system'
  | 'mcp' | 'usage' | 'tasks-skills' | 'agents' | 'routing';

// Props 新增
interface Props {
  // ... 现有 ...
  theme: Theme;
  onToggleTheme: () => void;
}
```

渲染分支（替换骨架占位）：
```tsx
{(view === 'settings' || view === 'system') && (
  <SettingsPanel
    runtimeConn={runtimeConn}
    desktopHost={desktopHost}
    apiKeyConfigured={apiKeyConfigured}
    theme={theme}
    onToggleTheme={onToggleTheme}
  />
)}
```

**Step 7 — `App.tsx` 适配**

从 `<Sidebar>` 移除 `theme` / `onToggleTheme` props，改为传入 `<RightPanel>`。

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
| `crates/config/src/lib.rs` | ~60 | — | — | +60 |
| `crates/tui/src/config.rs` | ~45 | ~5 | — | +50 |
| `commands.rs` | ~130 | — | — | +130 |
| `SettingsPanel.tsx` | ~210 | — | — | +210 |
| `RightPanel.tsx` | ~20 | ~10 | ~35 | -5 |
| `Sidebar.tsx` | ~5 | ~10 | ~40 | -25 |
| `App.tsx` | ~5 | ~5 | ~5 | +0 |
| `client.ts` | ~30 | — | — | +30 |
| `i18n/keys.ts` | ~42 | — | — | +42 |
| `i18n/zh-Hans.ts` | ~38 | — | — | +38 |
| `i18n/en.ts` | ~38 | — | — | +38 |
| `session_manager.rs` | — | ~3 | ~3 | +0 |
| **合计** | **~625** | **~33** | **~83** | **~570** |

> 比原计划多约 100 行，主要增量来自 TUI `Config` 双轨同步字段和 `SessionConfig`。

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
