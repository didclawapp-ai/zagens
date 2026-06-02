# LHT Round 2 — LabelMakePro Electron→Tauri（`label_rust`）

> **前置：** Round 1 已完成 `src-tauri/` 脚手架、`cargo check` 绿、`npm test` 30/30、`electron/` 已删、`frontend/src/tauri-api.ts` 已接。  
> **缺口：** `commands.rs` 中 **43 个**命令仍为 `not_impl()`；`adapters/`、`sync.rs` 在磁盘但未 `mod` 进 `lib.rs`；未跑 `npm run build` / `cargo tauri build`。  
> **用法：** 新建会话或同线程 steer — 整段复制下方「开场指令」+ 用 `checklist_write` 写入清单。

---

## 开场指令（复制给模型）

```
Round 2 — LabelMakePro Tauri 迁移补全（接续 F:\label_rust Round 1）

Round 1 已交付：src-tauri 脚手架、cargo check 绿、npm test 30/30、electron/ 已删、tauri-api.ts 桥接。

本轮目标：消灭 commands.rs 里所有 not_impl() 壳；启用 adapters + sync；前端 production build + Tauri 打包可过。

纪律：
- 不要 scratchpad_init / 不要全库 audit / 不要 spawn explore 子代理（除非单文件 >800 行且你先说理由）
- 每项 checklist 必须有 [verify: …]；禁止「创建文件」式验收
- 每完成一组命令，先 cargo check 再勾 completed
- 禁止新增 not_impl() / todo!() / unimplemented!() 糊弄编译
- 参考原 Electron：electron/main/database.ts、ipc-handlers.ts、adapters/、sync-manager.ts（若目录已删，用 git show HEAD:electron/... 或备份）

按下面 Round 2 清单逐项 update_plan + checklist_write，从 P0 开始。全部 P0 verify 绿后再动 P1。
```

---

## Round 2 清单（`checklist_write` 用）

### P0 — 数据库命令补全（13 项 → 接 `database.rs`）

1. **Templates CRUD** — `db_templates_get_all/get/get_preview/create/delete` 接 SQLite（对照原 `database.ts` templates 表）
   - `[verify: cd src-tauri && cargo check]`

2. **Template categories** — `db_template_categories_*` 六条 + `bulk_save`
   - `[verify: cd src-tauri && cargo check]`

3. **Settings** — `db_settings_get/set/get_all`
   - `[verify: cd src-tauri && cargo check]`

4. **Projects** — `db_projects_get_by_type/get/create/update/delete`
   - `[verify: cd src-tauri && cargo check]`

5. **Data sources** — `db_data_sources_*` 七条 + `test_draft_connection` + `preview_file_draft`
   - `[verify: cd src-tauri && cargo check]`

6. **DB maintenance** — `db_backup` / `db_restore` / `db_migrate_database`
   - `[verify: cd src-tauri && cargo check]`

### P0 — 后端模块接线（3 项）

7. **启用 adapters** — `lib.rs` 取消 `// mod adapters;`，补 `async-trait`、`url` 等 Cargo 依赖；`commands` 或 `sync` 调用 file/rest/mes 适配器
   - `[verify: cd src-tauri && cargo check]`

8. **启用 sync** — 取消 `// mod sync;`，修复 ErpAdapter dyn 兼容；实现 `sync_*` 五条命令（非 not_impl）
   - `[verify: cd src-tauri && cargo check]`

9. **ERP + MES 命令** — `erp_products_*`、`erp_orders_*`、`mes_work_orders_pending`、`mes_callback_*` 四条有真实逻辑或委托 sync/adapters
   - `[verify: cd src-tauri && cargo check]`

### P0 — 构建与打包（2 项）

10. **前端 production build** — Vite 在 Tauri 模式下 bundle 成功
    - `[verify: cd frontend && npm run build]`

11. **Tauri 打包路径** — release 编译 + tauri build 至少到 compile 阶段无 error
    - `[verify: cd src-tauri && cargo build --release]`
    - 若环境允许：`[verify: cargo tauri build --debug]`（或文档说明 blocker）

### P1 — 运行时与 IPC 对齐（4 项）

12. **tauri-api.ts 覆盖** — 对照 `lib.rs` invoke_handler 列表：每个已注册命令在桥接层有对应 `invoke` 封装；删除调用已 stub 命令的前端路径或改报错
    - 手动：grep `invoke(` in `frontend/src/tauri-api.ts` 数量 ≥ 已实现 Rust 命令数

13. **Update 命令** — `get_update_status` 接 `tauri-plugin-updater` 或诚实返回 structured not-ready（**禁止** not_impl 壳）
    - `[verify: cd src-tauri && cargo check]`

14. **Print 打印机列表** — `print_get_printers` 非空数组 stub（Windows: 枚举系统打印机或文档化限制）
    - `[verify: cd src-tauri && cargo check]`

15. **路径安全复验** — audit 报告 H-01~H-04：clipart/image_storage/file 适配器路径遍历 + rest SSRF 在 Rust 层有 `canonicalize`/hostname 校验
    - `[verify: cd src-tauri && cargo check]`
    - 手动：grep `path.resolve` / block private IP in `adapters/rest.rs`

### P1 — 收尾（2 项）

16. **零 not_impl** — `commands.rs` 中 `not_impl()` 调用数为 0
    - `[verify: grep -c not_impl src-tauri/src/commands.rs]`（harness 在 Windows 上会原生执行，无需 bash/grep）
    - 或：`[verify: cd src-tauri && cargo check]`（与编译门禁等价兜底）

17. **根目录 test 仍绿** — 不破坏 Round 1 的 npm test 委托
    - `[verify: npm test --silent]`

---

## 完成定义（strict LHT）

全部 P0 项 completed 且对应 verify exit 0 后，才允许 prose 收尾。预期 integration/manifest 门会再跑：

- `toolchain_cargo_check` / `toolchain_npm_test`
- 无 `electron/`、存在 `tauri-api.ts`（已满足则不应再 integration nudge）

若 checklist 100% 但仍有 `not_impl()` → **假绿**；不得 mark 完成。

---

## 建议不要在本轮做的

- 全库 CODE_AUDIT 报告更新
- 4 路 implementer 并行重写 `commands.rs`（单线程按 P0 分组更稳）
- `website/`、`tools/license-generator/` 范围外

---

## 参考路径

| 资源 | 路径 |
|------|------|
| Round 1 产物 | `F:\label_rust\src-tauri\` |
| 桥接层 | `frontend/src/tauri-api.ts` |
| 通用 round-2 模板 | [`lht-refactor-round2-checklist.md`](./lht-refactor-round2-checklist.md) |
| Harness 日志 | `%USERPROFILE%\.zagens\logs\sidecar.log`（thread `thr_3658ee8d`） |
