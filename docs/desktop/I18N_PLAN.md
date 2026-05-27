# Zagens 多语言方案

> 状态：Phase 2 已落地（zh-Hans / en / ja / pt-BR）
> 日期：2026-05-14  
> 作者：AI Assistant（经 Hmbown 审核）  
> 范围：`crates/desktop/web-ui/`

---

## 1. 目标

为 Zagens 桌面端增加中英文切换能力，后续可扩展其他语言。

- **Phase 1**（已完成）：简体中文（zh-Hans）+ 英语（en）
- **Phase 2**（已完成）：日语（ja）、巴西葡萄牙语（pt-BR）— 与 Runtime/TUI 已有 locale 对齐
- **Phase 3**（后续）：韩语、繁体中文等按需添加

## 2. 现状分析

### 2.1 文本分布统计

通过 `grep` 扫描 `web-ui/src/` 下所有 `.tsx` / `.ts` 文件中的中文字符，估算现有硬编码文本量：

| 文件 | 预估条目 | 热点类型 |
|------|---------|---------|
| `App.tsx` | ~60 | banner 错误提示、TitleBar 按钮 aria-label、线程信息、键盘快捷键描述 |
| `components/Composer.tsx` | ~40 | placeholder、工作区面板、附件提示、模式标签、按钮文本 |
| `components/AutomationPanel.tsx` | ~30 | 任务/技能表单、状态标签、提示文本 |
| `components/McpPanel.tsx` | ~15 | MCP 配置面板、删除确认、成功提示 |
| `components/ApiKeyForm.tsx` | ~12 | API Key 表单标签、说明文本、按钮 |
| `components/Sidebar.tsx` | ~10 | 导航按钮文本、会话操作 |
| `components/RightPanel.tsx` | ~10 | 面板标题、工作区标签 |
| `components/AgentPanel.tsx` | ~8 | 代理状态标签、统计卡片 |
| `components/ApprovalDialog.tsx` | ~5 | 审批对话框 |
| `components/ChatView.tsx` | ~3 | 欢迎界面文本 |
| `components/RoutingPanel.tsx` | ~5 | 路由面板 |
| `components/ModelParamsDialog.tsx` | ~3 | 模型参数对话框 |
| `其他组件` | ~10 | ToolCard、MessageBubble 等 |
| `api/client.ts` | ~3 | 网络错误提示 |

**总计约 200 条可翻译文本。**

### 2.2 现有基础设施

- **无** i18n 框架依赖（`package.json` 中没有 react-i18next、react-intl、i18next 等）
- **无** 翻译文件或语言配置文件
- 所有 UI 文本直接硬编码在 JSX 中，混合了大量动态变量拼接（模板字符串 ` \`...${var}...\` `）
- `types/desktop.ts` 中已有少量英文常量定义（如 `DESKTOP_MODEL_LABELS`、`DESKTOP_RUN_MODE_LABELS`），可作为接入点

### 2.3 约束条件

- **零外部依赖**：不引入 react-i18next / i18next 等第三方库。Zagens 是 Tauri 桌面应用，不需要 Web 生态的重量级 i18n 框架
- **构建时不变**：翻译表在 Vite 构建时静态解析，无运行时网络请求
- **不影响包体积**：方案应支持按需加载语言包
- **TypeScript 类型安全**：翻译 key 应有类型提示，避免拼写错误
- **最小侵入**：优先考虑 Provider + hook 模式，不改动现有组件结构

### 2.4 关联项目：TUI 本地化现状

TUI 层（`crates/tui/src/localization.rs`）已有完整的本地化系统，架构如下：

- **翻译 key**：`MessageId` 枚举（200+ 变体），编译期保证完整性
- **支持语言**：`en` / `ja` / `zh-Hans` / `pt-BR`（4 种）
- **查表函数**：`tr(locale, id) -> &'static str`，缺失 key 自动回退英语
- **语言解析链**：`config.toml` 的 `locale` 设置 → 环境变量（LC_ALL / LC_MESSAGES / LANG）→ 英语兜底
- **插值语法**：`{placeholder}`（与桌面方案提议的 `{{placeholder}}` 略有差异，但语义等价）

**关键结论**：TUI 与桌面的翻译文本几乎无重叠（两端 UI 完全不同）。决定保持**两套独立翻译表，仅语言标签对齐**（使用相同的 BCP-47 标签：`en`、`zh-Hans`、`ja`、`pt-BR`），确保 `config.toml` 中的 `locale` 设置在两端行为一致。

---

## 3. 架构设计

### 3.1 整体方案：React Context + Vite 按需加载

```
src/
├── i18n/
│   ├── index.ts              ← 入口：导出 I18nProvider、useT、Locale 类型
│   ├── context.ts            ← React Context + Provider（含语言切换逻辑）
│   ├── keys.ts               ← 翻译 key 的 TypeScript 类型定义
│   ├── locales/
│   │   ├── zh-Hans.ts        ← 简体中文翻译表
│   │   └── en.ts             ← 英语翻译表
│   └── utils.ts              ← 插值工具、数字/日期格式化
```

### 3.2 核心组件

#### 3.2.1 `I18nProvider`

```tsx
// 包裹 App 根组件
<I18nProvider defaultLocale="zh-Hans">
  <App />
</I18nProvider>
```

- 从 `localStorage` 读取用户上次选择的语言
- 默认值：`zh-Hans`
- 提供 `setLocale(locale)` 方法触发全局重渲染

#### 3.2.2 `useT()` hook

```tsx
const { t } = useT();

// 简单文本
t('sidebar.newSession')           // → "新对话" / "New Chat"

// 带插值
t('banner.loadSessionsError', { message: err.message })
// → "无法加载会话列表：Network Error"

// 带复数（简单形式）
t('agentCount', { count: 5 })
// → "5 agents running" / "5 个子代理运行中"
```

**不使用模板字符串拼接**。所有含变量的文本通过 key + params 方式，翻译表用 `{{placeholder}}` 占位。

#### 3.2.3 翻译表格式

```typescript
// locales/zh-Hans.ts
const zhHans = {
  app: {
    title: 'Zagens',
    subtitle: '你的 AI 编码助手',
  },
  sidebar: {
    newSession: '新对话',
    workspace: '工作台',
    apiKey: 'API Key',
    settings: '设置',
    mcp: 'MCP',
    tasksSkills: '任务 / 技能',
    agents: '子代理',
    routing: '路由',
    themeLight: '浅色',
    themeDark: '深色',
    deleteConfirm: '确定删除此会话？',
  },
  composer: {
    placeholder: '今天需要什么帮助？（可粘贴截图）',
    workspaceLabel: '工作区目录',
    send: '发送',
    stop: '停止',
    chooseWorkspace: '选择工作区目录',
    browseFolder: '浏览文件夹…',
    // ... 更多
  },
  banner: {
    unauthorized: '未授权：运行时 token 无效...',
    loadSessionsError: '无法加载会话列表：{{message}}',
    // ... 更多
  },
  // ... 更多命名空间
} as const;

export default zhHans;
```

```typescript
// locales/en.ts
import type { TranslationMap } from '../keys';

const en: TranslationMap = {
  app: {
    title: 'Zagens',
    subtitle: 'Your AI coding assistant',
  },
  sidebar: {
    newSession: 'New Chat',
    workspace: 'Workspace',
    // ... 一一对应
  },
  // ...
} as const;

export default en;
```

#### 3.2.4 TypeScript 类型约束

```typescript
// keys.ts
import type zhHans from './locales/zh-Hans';

// 从中文翻译表提取键路径类型
export type TranslationMap = typeof zhHans;

// 深度键路径类型（支持 t('sidebar.newSession') 这样的点号访问）
type DotPrefix<T extends string, K extends string> = K extends string
  ? `${T}.${K}`
  : never;

export type TranslationKey = DotPaths<TranslationMap>;

// 插值参数类型（从值的 {{placeholder}} 提取）
export type TranslationParams<K extends TranslationKey> = /* ... */;
```

#### 3.2.5 语言检测与持久化

```
优先级：localStorage（用户手动选择）> navigator.languages / navigator.language > 默认 en
```

未提供语言包的语言（如 zh-TW、de、fr）回退到 **English**，不再默认简体中文。

```typescript
// utils.ts — 系统语言匹配示例
matchLocaleFromTag('zh-CN')  // → 'zh-Hans'
matchLocaleFromTag('zh-TW')  // → null（无繁体包 → 最终 en）
matchLocaleFromTag('ja-JP')  // → 'ja'
matchLocaleFromTag('de-DE')  // → null → en
```

### 3.3 动态文本（模板字符串）处理策略

当前代码中大量使用模板字符串拼接错误提示，如：

```tsx
setBanner(`无法加载会话列表：${err.message}`);
setBanner(`重试失败：${(e as Error).message}`);
```

**处理方式：**

```tsx
// 翻译表
banner: {
  loadSessionsError: '无法加载会话列表：{{message}}',
  retryFailed: '重试失败：{{message}}',
}

// 使用
setBanner(t('banner.loadSessionsError', { message: err.message }));
```

对于更复杂的多段拼接（含 HTML 片段的），如：

```tsx
setBanner(
  `无法连接本地运行时（${getRuntimeBase()}）。本地服务可能仍在启动，请点击「重试连接」；...`
);
```

处理为：

```tsx
// 翻译表
banner: {
  runtimeUnreachable: '无法连接本地运行时（{{url}}）。本地服务可能仍在启动，请点击「重试连接」；若多次失败请重启应用或检查是否已内置 sidecar。',
}

// 使用
setBanner(t('banner.runtimeUnreachable', { url: getRuntimeBase() }));
```

---

## 4. 实施计划

### Phase 1：基础设施（2 个文件，~100 行）

| # | 步骤 | 产物 |
|---|------|------|
| 1.1 | 创建 `src/i18n/keys.ts` — 英语翻译表类型定义 | 类型基础 |
| 1.2 | 创建 `src/i18n/locales/en.ts` — 英语翻译表（所有 ~200 条目，值与中文对应） | 英语包 |
| 1.3 | 创建 `src/i18n/locales/zh-Hans.ts` — 中文翻译表（从现有代码迁移所有硬编码文本） | 中文包 |
| 1.4 | 创建 `src/i18n/context.ts` — `I18nProvider` + `useT` + 语言切换逻辑 | React 上下文 |
| 1.5 | 创建 `src/i18n/utils.ts` — 语言检测、插值、简单格式化 | 工具函数 |
| 1.6 | 创建 `src/i18n/index.ts` — 导出所有公共 API | 入口 |

### Phase 2：组件迁移（逐文件，~13 个文件）

按优先级从高到低：

| # | 文件 | 文本条数 | 复杂度 | 说明 |
|---|------|---------|--------|------|
| 2.1 | `App.tsx` | ~60 | 🔴 高 | banner 文本大量模板字符串；TitleBar aria-label；线程信息 |
| 2.2 | `Composer.tsx` | ~40 | 🔴 高 | placeholder、工作区面板、附件提示、运行模式标签、按钮文本 |
| 2.3 | `AutomationPanel.tsx` | ~30 | 🟡 中 | 表单标签、状态映射表、提示文本 |
| 2.4 | `McpPanel.tsx` | ~15 | 🟡 中 | 配置面板、确认对话框 |
| 2.5 | `Sidebar.tsx` | ~10 | 🟢 低 | 导航按钮文本 |
| 2.6 | `RightPanel.tsx` | ~10 | 🟢 低 | 面板标题、标签 |
| 2.7 | `ApiKeyForm.tsx` | ~12 | 🟡 中 | 表单、placeholder |
| 2.8 | `ApprovalDialog.tsx` | ~5 | 🟢 低 | 对话框文本 |
| 2.9 | `AgentPanel.tsx` | ~8 | 🟢 低 | 状态标签 |
| 2.10 | `ChatView.tsx` | ~3 | 🟢 低 | 欢迎界面 |
| 2.11 | `RoutingPanel.tsx` | ~5 | 🟢 低 | 路由面板 |
| 2.12 | `ModelParamsDialog.tsx` | ~3 | 🟢 低 | 对话框 |
| 2.13 | 其他组件 | ~10 | 🟢 低 | ToolCard、MessageBubble 等 |

### Phase 3：语言切换 UI

| # | 步骤 |
|---|------|
| 3.1 | 在 Sidebar 底部增加语言切换控件（下拉或按钮组） |
| 3.2 | `main.tsx` 中包裹 `I18nProvider` |
| 3.3 | 构建验证 `tsc -b && vite build` |

### Phase 4：验证

| # | 步骤 |
|---|------|
| 4.1 | 手动切换中英文，逐个面板检查文本完整性 |
| 4.2 | 检查无 key-not-found 运行时错误（所有 key 有类型约束，编译期保证） |
| 4.3 | Tailwind 长文本截断检查（英文通常比中文长 30-50%） |

---

## 5. 设计决策

### 5.1 为什么不用 react-i18next

| 方面 | react-i18next | 自建方案 |
|------|-------------|---------|
| bundle 增量 | ~40 KB（含 i18next + react-i18next） | ~3 KB |
| 学习曲线 | Provider、Trans、useTranslation、t、Interpolate、复数规则... | 一个 `useT()` hook |
| 类型安全 | 需额外 `react-i18next.d.ts` 增强 | 原生 TypeScript 推导 |
| 构建集成 | 需 `i18next-parser` 等提取工具 | Vite 原生静态 import |
| 依赖链 | i18next → 5 个子包 | 零依赖 |

Zagens 的翻译需求是 **静态 UI 文本 + 简单插值**，不需要复数规则、日期格式化、ICU MessageFormat 等复杂特性。

### 5.2 为什么从中文翻译表推导类型

```typescript
// 以中文为 source of truth
type TranslationMap = typeof zhHans;

// 英语必须完全实现 TranslationMap
const en: TranslationMap = { ... };
```

**优点：**
- 中文是默认语言，始终完整
- 英语的类型错误在编译期暴露（缺少 key → TypeScript 报错）
- 添加新语言时，TypeScript 强制要求覆盖所有 key

### 5.3 关于 `types/desktop.ts` 中现有常量

`DESKTOP_MODEL_LABELS`、`DESKTOP_RUN_MODE_LABELS` 等已是英文常量。这些是**模型 ID → 显示名**的映射，无论用户选什么语言，显示名都保持原样。因此这部分**不迁移**到翻译表。

### 5.4 关于 Tailwind CSS 长文本

英文译文通常比中文长 1.3-1.5 倍。需要注意：

- `placeholder` 文本可能超出输入框
- 按钮文本可能换行
- Sidebar 导航文本可能截断

措施：
- 翻译时控制英文长度，优先简洁表达
- 关键位置使用 `truncate` / `whitespace-nowrap` + tooltip

### 5.5 关于代码注释和开发者文本

以下类型文本**不翻译**，保持原样：
- JSX 注释（`{/* ... */}`）
- JSDoc 注释
- 控制台日志
- API 端点路径
- CSS 类名
- `console.error` 消息

---

## 6. 翻译表 Key 命名规范

采用 **命名空间 + 点分路径**：

```
{domain}.{subdomain}.{key}

示例：
sidebar.newSession
composer.placeholder
banner.unauthorized
approval.title
automation.tabs.tasks
```

**规则：**
1. 命名空间与组件或功能域对应（`sidebar`、`composer`、`banner`、`approval` …）
2. 子域用于区分同一组件内的不同区域（如 `automation.tabs.tasks` vs `automation.form.taskPrompt`）
3. key 使用 camelCase
4. 不嵌套超过 3 层

---

## 7. 实现示例

### 7.1 Provider 使用

```tsx
// main.tsx
import { I18nProvider } from './i18n';

async function bootstrap() {
  await initRuntimeConfig();
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <I18nProvider defaultLocale="zh-Hans">
        <App />
      </I18nProvider>
    </React.StrictMode>,
  );
}
```

### 7.2 组件内使用

```tsx
// Sidebar.tsx
import { useT } from '../i18n';

export default function Sidebar({ ... }) {
  const { t, locale, setLocale } = useT();

  return (
    <nav>
      <button onClick={onNewSession}>
        {t('sidebar.newSession')}
      </button>
      <button onClick={() => onInspectorChange('workspace')}>
        {t('sidebar.workspace')}
      </button>
      {/* ... */}

      {/* 语言切换 */}
      <select value={locale} onChange={e => setLocale(e.target.value as Locale)}>
        <option value="zh-Hans">中文</option>
        <option value="en">English</option>
      </select>
    </nav>
  );
}
```

### 7.3 模板字符串迁移

```tsx
// 迁移前
setBanner(`无法加载会话列表：${err.message}`);

// 迁移后
const { t } = useT();
setBanner(t('banner.loadSessionsError', { message: err.message }));
```

### 7.4 i18n 不受 state 更新丢失影响

`I18nProvider` 使用 React Context，语言切换触发所有消费组件重渲染。`useT()` 返回的 `t` 函数闭包捕获最新 `locale`，不存在状态不同步问题。

由于所有文本都通过 `t()` 调用获得，切换语言后每个组件自动显示新文本，**无需额外处理**。

**例外**：如果某段文本被赋给 `useState` 初始值（如 `const [banner, setBanner] = useState('...')`），切换语言时不会自动更新。遇到此类情况，直接在渲染时调用 `t()` 而不是存入 state，或将整个 state 改为 key + params 结构。

针对 `App.tsx` 中的 `banner` state：
```tsx
// 当前
const [banner, setBanner] = useState<string | null>(null);

// 建议改为存 key + params
const [bannerKey, setBannerKey] = useState<{key: string; params?: Record<string,string>} | null>(null);

// 渲染
{bannerKey && <div>{t(bannerKey.key, bannerKey.params)}</div>}
```

---

## 8. 后续扩展

### 8.1 添加新语言

1. 复制 `locales/en.ts` → `locales/ja.ts`（或其他目标语言）
2. 翻译所有值，TypeScript 自动校验完整性
3. 在 `I18nProvider` 的 `Locale` 类型中添加对应标签
4. 在语言选择器中添加选项

**日/葡优先**：TUI 已完整支持 `ja`（日语）和 `pt-BR`（葡萄牙语），桌面可快速对齐，在 `config.toml` 中切换 `locale` 时两端体验一致。

### 8.2 可能需要的增强

- **日期/时间本地化**：使用 `Intl.DateTimeFormat`（浏览器原生），不引入 moment/dayjs
- **数字格式化**：使用 `Intl.NumberFormat`（浏览器原生）
- **动态语言包加载**：`import()` 动态导入（如果语言包变大）

---

## 9. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 翻译遗漏 | TypeScript 类型强制每个 key 都有英文对应值 |
| 文本过长导致 UI 溢出 | 翻译时控制长度；关键位置加 `truncate` |
| 模板字符串迁移疏漏 | Phase 2 逐文件迁移 + 全局 grep 验证无残留中文 |
| `useT` 未覆盖的路径（如 `api/client.ts` 中的纯字符串） | `api/client.ts` 的错误消息保持英文，不翻译 |
| 切换语言后部分 state 不更新 | 见 7.4 节处理策略 |

---

## 10. 时间估算

| 阶段 | 预估工时 |
|------|---------|
| Phase 1：基础设施 | 1-2 小时 |
| Phase 2：组件迁移 | 2-4 小时（200 条文本，逐条迁移 + 翻译） |
| Phase 3：UI + 集成 | 0.5 小时 |
| Phase 4：验证 + 修复 | 1 小时 |
| **合计** | **4.5-7.5 小时** |

---

## 附录 A：现有文本扫描完整清单

> 见 `docs/desktop/I18N_STRINGS_INVENTORY.md`（下一步产出）

## 附录 B：参考

- Tauri v2 内置的 `os.locale()` 可用于 Rust 侧检测系统语言
- macOS 托盘菜单文本可能需要单独处理（`main.rs` 中 `tauri::menu` 的文本）
- 后端错误消息（`commands.rs` 返回的 `Err` 字符串）暂不翻译，保持英文
