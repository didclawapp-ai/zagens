# 工作台「目录」Tab — 方案与实施跟踪

> **状态：** 实施中 — 阶段 A/B/D、C1/C2/C3、A6、E3（变更筛选）已落地（2026-05-21）  
> **范围：** DS Pick 右侧面板 · 工作台 · **目录** 子 Tab（`RightPanel` `workspaceTab === 'files'`）  
> **目标：** 提升 monorepo 工作区浏览效率，强化与预览 / 对话 / Diff / 审计场景的联动；不替代 Composer 的「选择工作区」职责。  
> **相关：** [DEV_NOTES.md](DEV_NOTES.md)、[PREVIEW_ARCHITECTURE.md](PREVIEW_ARCHITECTURE.md)、[docs/tech/API_DESIGN.md](../tech/API_DESIGN.md) § workspace browse、[TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md)

**图例：** ✅ 已落地 · 🔶 部分 / 雏形 · ⬜ 未做 · ❌ 明确不做（本版）

---

## 0. 实施总览（维护者更新此表）

| 阶段 | 主题 | 状态 | 目标版本 / 备注 |
|------|------|------|-----------------|
| **A** | 基础体验（滚动、工具栏、路径区、i18n） | ✅ | `WorkspaceFilesPanel` + `FlatIcons` |
| **B** | 过滤与噪音控制（搜索、忽略目录） | ✅ | 全工作区搜索 + denylist + 显示隐藏；B4 虚拟列表 |
| **C** | 联动（预览高亮、对话定位、Diff 跳转） | ✅ | C1–C3：`focusFilesRelPath`、对话/ Diff「在目录中显示」 |
| **D** | 树形懒加载（可选） | ✅ | `WorkspaceFileTree` + `useWorkspaceDirCache` |
| **E** | Agent 增强（批量 @、审计筛选、敏感路径提示） | 🔶 | E1/E4 ✅；E3 Office 预设 ✅；E2 ⬜ |
| **F** | 后端增强（大目录上限、可选 git 状态） | ⬜ | `runtime_api.rs` |

**最近更新：** 2026-05-21 — C2/C3 对话与 Diff「在目录中显示」、定位滚入视口、A6 键盘、`/` 搜索、E3 Office 目录预设（含本轮变更）。

---

## 1. 问题陈述

### 1.1 现象（用户侧）

在大型仓库（如本 monorepo 含 `target/`、`vendor/`、多层 dot 目录）中，目录 Tab 呈现为**扁平单层列表**：

- 路径信息重复（Composer 工作区 + 解析路径常相同）；
- 面包屑段名截断过短（`max-w-[7rem]`），深层路径难辨认；
- 无当前层搜索，大目录需反复点击进入；
- 与 VS Code / Cursor 资源管理器相比，缺少「忽略构建产物」「树形展开」等习惯能力；
- 右键能力存在但不够显眼（「添加至对话」依赖发现成本）。

### 1.2 现状（代码锚点，2026-05-21）

| 能力 | 状态 | 位置 |
|------|------|------|
| 单层 browse API | ✅ | `GET /v1/workspace/browse`、`GET /v1/threads/{id}/workspace/browse` → `read_dir_sorted` |
| 目录优先 + 名称排序 | ✅ | `crates/tui/src/runtime_api.rs` `read_dir_sorted` |
| 根下隐藏 `.git` | ✅ | 同上（非 `.git` 目录内仍列出） |
| 面包屑 | ✅ | `pathBreadcrumbs` · `RightPanel.tsx` |
| 点击文件预览 | ✅ | `onOpenFileFromTree` → `openWorkspaceFile` |
| 右键菜单 | ✅ | 复制绝对/相对路径、添加至对话、资源管理器、部分扩展名系统打开 |
| 打开工作区根（Shell） | ✅ | 面板头「打开文件夹」· `open_in_shell` |
| 切换 Tab 聚焦目录 | ✅ | `focusFilesNonce` |
| 线程 vs Composer 路径 | ✅ | `browseThreadWorkspace` / `browseComposerWorkspace` |
| Office 默认 `deliverables` | ✅ | `officeSession` effect |
| 文案 i18n | ✅ | `workspaceFiles.*`、`workbench.*`、`panels.*` |
| 列表独立滚动 | 🔶 | 需确认 `overflow-y-auto` + `min-h-0` |
| 刷新按钮 | 🔶 | `browseNonce` 已有，UI 未暴露 |
| 搜索 / 树 / git 状态 | ⬜ | — |

### 1.3 非目标（本方案）

- **不**在目录 Tab 重复 Composer 的「选择工作区 / 改根路径」（仅只读浏览 + 引用）。
- **不**在 WebView 内直接拼未校验的绝对路径访问磁盘（继续走 runtime browse + 现有 path guard）。
- **不**一期实现完整 IDE 级文件树（重命名、删除、新建文件）—— 写操作仍由 Agent 工具链承担。
- **不**替代 TUI 内置 `@` 文件选择器；桌面目录 Tab 是**可视化补充**。

---

## 2. 设计原则

1. **Browse 只读、引用优先** — 打开预览、复制路径、`@` 进对话；变更磁盘走 runtime 工具与审批。
2. **噪音可控** — 默认隐藏或折叠 `target`、`node_modules`、常见 dot 工具目录；用户可「显示隐藏项」。
3. **与 Composer 分工清晰** — 换工作区 = Composer；浏览与定位 = 目录 Tab。
4. **联动胜于孤立树** — 预览、Diff、消息内路径、审计 deliverables 应能**一跳定位**到目录项。
5. **小步可验收** — 分阶段交付；每阶段可在 [audit-scratchpad-test.md](audit-scratchpad-test.md) 式 checklist 或本文件 §7 打勾。

---

## 3. 信息架构与文案

### 3.1 路径区（合并重复）

**现状：** 固定展示「Composer 工作区」+ 全路径；若 `browseWorkspace` 存在再展示「解析路径」。

**目标：**

| 条件 | UI |
|------|-----|
| `browseWorkspace` 为空或与 `workspaceRoot` 相同 | 单行：`工作区` + mono 路径 + 复制按钮 |
| 线程解析路径不同 | 主行 Composer 路径；副行「线程工作区：…」 |
| 未设置工作区 | 与 Composer 一致的提示 + 链到 Composer 设置（文案即可，不必深链接） |

**状态：** ⬜ A1

### 3.2 面包屑

- 容器 `overflow-x-auto`，末段不截断或放宽 `max-w`；
- `title` 保留全路径；
- 可选：首段「根」图标 + 末段 `font-medium`。

**状态：** ⬜ A2

### 3.3 国际化

将目录 Tab 硬编码字符串迁入 `workspaceFiles.*`（`keys.ts` / `zh-Hans.ts` / `en.ts`），与 `workspaceRules`、`diff`、`terminal` 命名空间并列。

**状态：** ✅ A3

---

## 4. UI 与交互

### 4.1 列表区布局

```
┌─ 路径区（§3）─────────────────────────────┐
├─ 工具栏：上级 | 刷新 | 在资源管理器打开当前目录 ┤
├─ 搜索框（§5.1，阶段 B）───────────────────┤
├─ 面包屑 ─────────────────────────────────┤
├─ 文件列表（scroll）──────────────────────┤
└─ 空态 / 错误 / 加载 ─────────────────────┘
```

| 项 | 说明 | 状态 |
|----|------|------|
| 列表 `overflow-y-auto` + flex `min-h-0` | 避免长列表撑破右栏 | ⬜ A4 |
| 文件类型图标 | 文件夹 vs 扩展名弱色（可用内联 SVG 或轻量映射表） | ⬜ A5 |
| 预览中文件高亮 | 从 `preview` / 最近打开路径 prop 传入 | ⬜ C1 |
| 键盘：Enter / Backspace | 打开或进入；返回上级 | ⬜ A6（可选） |

### 4.2 工具栏

| 操作 | 行为 | 状态 |
|------|------|------|
| 上级 | `browseRelPath` 去掉最后一段 | ⬜ A7 |
| 刷新 | `setBrowseNonce(n => n+1)` | ⬜ A8 |
| 打开当前目录 | `open_in_shell` 目标为 `absPath(browseRelPath)` 或根 | ⬜ A9 |

面板头「打开文件夹」保留为**工作区根**；工具栏为**当前浏览目录**。

### 4.3 右键与行内操作

保留现有菜单，增加：

| 项 | 状态 |
|----|------|
| 行尾 hover「+」→ `openWorkspaceFile` / 添加至对话（与右键等价） | ⬜ E1 |
| 多选 + 批量复制相对路径（Composer `@` 格式） | ⬜ E2 |

---

## 5. 功能：过滤与噪音

### 5.1 工作区文件搜索（阶段 B，原「当前层筛选」已升级）

- **同一输入框**：有内容时全工作区 BFS 搜索（`browse` API，跳过 denylist 目录）；无内容时正常浏览；
- 防抖 320ms；最多 200 条结果 / 1200 目录扫描；`Esc` 清空；
- 快捷键 `/` 聚焦搜索框。

**状态：** ✅ B1（2026-05-21 升级）

### 5.2 默认折叠 / 隐藏目录

**前端 denylist（建议首版，可配置常量）：**

```
node_modules, target, vendor, dist, build, .git,
.cursor, .deepseek, .trae, .claude, .github/actions/cache
```

| 模式 | 行为 | 状态 |
|------|------|------|
| 默认 | 列表不展示 denylist 名称（仍可通过搜索「显示」若开启包含隐藏） | ⬜ B2 |
| 「显示隐藏项」开关 | `localStorage['ds-pick-dir-show-hidden']` | ⬜ B3 |
| 后续 | 读取工作区 `.gitignore` 合并规则（需 runtime 或 Tauri 读文件） | ⬜ F2 |

**后端（阶段 F，可选）：** `read_dir_sorted` 增加 `?hide=` 或内置 ignore，减少 payload。

**状态：** ⬜ B2 / F1

### 5.3 大目录

| 策略 | 状态 |
|------|------|
| 前端：条目 > 500 时虚拟列表（`@tanstack/react-virtual` 或自研简单 window） | ⬜ B4 |
| 后端：`read_dir` 超过 N 条截断 + `truncated: true` 字段（API 扩展） | ⬜ F3 |

---

## 6. 联动

### 6.1 预览 ↔ 目录

- `App` 将当前预览相对路径传入 `RightPanel`；
- 若预览路径不在当前 `browseRelPath` 祖先链，自动 `setBrowseRelPath` 到父目录并高亮文件行。

**状态：** ⬜ C1

### 6.2 对话内路径 → 目录

- `ChatMarkdown` / `workspaceLinkMenu` 已有 `normalizeWorkspaceRelPath`；
- 新增：`focusWorkspaceFilesNonce` 携带 `relPath`（扩展 prop 或 sessionStorage 一次消费）；
- 打开工作台目录 Tab 并定位。

**状态：** ⬜ C2

### 6.3 Diff Tab → 目录

- Diff 文件列表点击「在目录中显示」→ 切 `files` Tab + 设 `browseRelPath` 父路径 + 高亮。

**状态：** ⬜ C3

### 6.4 Office / 审计

- 保留进入 `deliverables` 默认；
- 工具栏筛选：`全部 | deliverables | docs | 本轮变更`（变更列表来自 scratchpad / diff API，需接口对齐）。

**状态：** ⬜ E3

### 6.5 敏感路径弱提示

对 `.env`、`credentials`、`*.pem` 等匹配模式显示 ⚠ 图标（不阻断打开）。

**状态：** ⬜ E4

---

## 7. 树形浏览（阶段 D，可选）

**模型：** 懒加载树；展开节点时调用现有 browse API（`path=` 相对路径），缓存 `Map<relPath, entries>`。

| 项 | 状态 |
|----|------|
| 抽出 `WorkspaceFileTree.tsx` | ✅ D1 |
| 展开/折叠状态 session 持久化（按工作区根 key） | ✅ D2 |
| 与扁平模式切换（设置或工具栏 toggle） | ✅ D3 |

**不做的：** 一次 API 返回整棵树。

---

## 8. API 与后端（参考）

### 8.1 现有契约

```http
GET /v1/workspace/browse?workspace={root}&path={rel}
GET /v1/threads/{id}/workspace/browse?path={rel}
```

响应（摘录）：

```json
{
  "workspace": "F:\\repo",
  "path": "crates/desktop",
  "entries": [{ "name": "src", "kind": "directory" }, { "name": "Cargo.toml", "kind": "file", "size": 1234 }]
}
```

实现：`browse_workspace_by_root` · `read_dir_sorted` · `ensure_workspace_browse_subdir` / `safe_thread_subpath`。

### 8.2 建议扩展（阶段 F）

| 扩展 | 用途 | 状态 |
|------|------|------|
| `truncated: boolean` + `total_count` | 大目录提示 | ⬜ F3 |
| `entries[].git_status?: "M"|"A"|"D"|"?"` | 与 git 或 diff 快照对齐 | ⬜ F4 |
| `?ignore=default` | 服务端应用 ignore 规则 | ⬜ F1 |

---

## 9. 涉及文件（实施时勾选）

| # | 文件 | 阶段 | 改动类型 |
|---|------|------|----------|
| 1 | `crates/desktop/web-ui/src/components/RightPanel.tsx` | A–E | 目录 Tab UI、工具栏、筛选 |
| 2 | `crates/desktop/web-ui/src/components/WorkspaceFileTree.tsx` | D | **新建**（可选） |
| 3 | `crates/desktop/web-ui/src/lib/workspaceBrowse.ts` | A–B | **新建**：denylist、filter、path 工具 |
| 4 | `crates/desktop/web-ui/src/App.tsx` | C | 预览路径、focus 携带 relPath |
| 5 | `crates/desktop/web-ui/src/components/DiffCard.tsx` 或 Diff 列表 | C | 「在目录中显示」 |
| 6 | `crates/desktop/web-ui/src/i18n/keys.ts` | A | `workspaceFiles.*` |
| 7 | `crates/desktop/web-ui/src/i18n/locales/zh-Hans.ts` | A | 文案 |
| 8 | `crates/desktop/web-ui/src/i18n/locales/en.ts` | A | 文案 |
| 9 | `crates/desktop/web-ui/src/api/client.ts` | F | 响应类型扩展 |
| 10 | `crates/tui/src/runtime_api.rs` | F | `read_dir_sorted` / 响应结构 |
| 11 | `docs/tech/API_DESIGN.md` | F | browse 响应文档 |
| 12 | `CHANGELOG.md` | 每阶段 | Unreleased 用户可见条目 |

---

## 10. 分阶段任务清单（复制到 PR / issue）

### 阶段 A — 基础体验

- [x] A1 路径区合并（Composer vs 解析路径）
- [x] A2 面包屑横向滚动与截断策略
- [x] A3 目录 Tab i18n（`workspaceFiles` + `workbench` + `panels`）
- [x] A4 列表独立滚动
- [x] A5 文件/文件夹图标（`FlatIcons` / `WorkspaceEntryIcon`）
- [x] A6 键盘导航（`/` 聚焦搜索、Backspace 上级）
- [x] A7 上级按钮
- [x] A8 刷新按钮
- [x] A9 打开当前目录（资源管理器）

### 阶段 B — 过滤与噪音

- [x] B1 当前层搜索框
- [x] B2 默认 denylist 隐藏
- [x] B3 「显示隐藏项」开关 + localStorage
- [x] B4 大列表虚拟滚动（≥48 条时窗口化渲染）

### 阶段 C — 联动

- [x] C1 预览文件高亮 + 自动展开父路径（`focusFilesRelPath`）
- [x] C2 对话内路径跳转目录 Tab（右键「在目录中显示」，左键仍打开预览）
- [x] C3 Diff 列表 → 目录定位

### 阶段 D — 树形（可选）

- [x] D1 `WorkspaceFileTree` 懒加载
- [x] D2 展开状态持久化
- [x] D3 扁平 / 树形切换

### 阶段 E — Agent / 审计

- [x] E1 行内「添加至对话」（hover `+`）
- [ ] E2 多选批量路径
- [x] E3 Office 目录筛选预设（全部 / deliverables / docs / 本轮变更）
- [x] E4 敏感路径弱提示

### 阶段 F — 后端（可选）

- [ ] F1 服务端 ignore / 条目上限
- [ ] F2 `.gitignore` 合并（前端或后端二选一）
- [ ] F3 `truncated` 响应字段
- [ ] F4 `git_status` 角标数据

---

## 11. 验收标准（手工）

| # | 场景 | 通过条件 |
|---|------|----------|
| R1 | 打开 monorepo 根目录 | 列表可滚动；`target` 默认不可见（B2 后） |
| R2 | 深层路径 `crates/desktop/web-ui/src` | 面包屑可辨认；点击任意段可跳转 |
| R3 | 点击 `Cargo.toml` | 预览打开；目录行高亮（C1 后） |
| R4 | 右键「复制相对路径」 | 剪贴板为工作区相对 POSIX 路径 |
| R5 | 运行时断开 | 与现有一致：提示连接 + 不可浏览 |
| R6 | 线程工作区 ≠ Composer | 解析路径副行展示正确 |
| R7 | Office 模式 | 默认进入 `deliverables`；筛选可用（E3 后） |
| R8 | 英文界面 | 目录 Tab 无硬编码中文（A3 后） |

---

## 12. 风险与约束

| 风险 | 缓解 |
|------|------|
| 隐藏 `target` 后用户找不到构建产物 | 「显示隐藏项」+ 搜索按名匹配 |
| 树形缓存与磁盘不同步 | 刷新按钮；切换工作区清空缓存 |
| i18n 遗漏 | `rg` 硬编码中文限定 `RightPanel` files tab |
| API 扩展破坏旧客户端 | 新字段 optional；旧 UI 忽略 |

安全：继续禁止 WebView 直接 `read_dir`；browse 路径经 `safe_thread_subpath` / `ensure_workspace_browse_subdir`。

---

## 13. 修订记录

| 日期 | 作者 | 摘要 |
|------|------|------|
| 2026-05-21 | — | 初稿：现状梳理、分阶段方案、实施跟踪表 |

---

## 附录 A — 与 Cursor / VS Code 差异（产品定位）

| 能力 | VS Code 资源管理器 | DS Pick 目录 Tab（目标） |
|------|-------------------|-------------------------|
| 改工作区根 | 文件 → 打开文件夹 | **Composer** |
| 编辑文件 | 内置编辑器 | **预览面板** + Agent 工具 |
| Git 装饰 | 内置 | 阶段 F / Diff 联动 |
| `@` 引用 | 无 | **添加至对话** 一等公民 |
| 审计 deliverables | 无 | Office 预设路径 |

## 附录 B — 现有右键菜单（保持）

| 菜单项 | 实现 |
|--------|------|
| 复制路径 | `ctxCopyAbs` |
| 复制相对路径 | `ctxCopyRel` |
| 添加至对话 | `ctxAddConv` → `openWorkspaceFile` |
| 在文件资源管理器打开 | `ctxOpenExplorer` |
| 用系统应用打开 | `ctxSystemOpen`（扩展名白名单） |
