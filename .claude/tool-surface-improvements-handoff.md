# 交接：工具面改进实施（基于 TOOL_SURFACE_AUDIT v1）

**日期：** 2026-05-31
**起点 commit：** `a1d57cb`（工作树 clean，**未 push**）
**权威清单：** [`docs/tech/TOOL_SURFACE_AUDIT.md`](../docs/tech/TOOL_SURFACE_AUDIT.md)（实证审计 v1，带 `文件:行号` + 严重度 + 是否已缓解）
**一句话：** MicroStack03 压测暴露最大弱点在**工具功能本身**。审计已完成并落档，本交接是**实施清单**——按 P0/P1 顺序改，每条都有定位、改法、测试位置与验证命令。审计文件是 backlog 快照,**改完一项回填「已缓解」+ 记 CHANGELOG**。

---

## 0. 接手第一步（确认环境）

```powershell
cd F:\DeepSeek-TUI-desktop
git log --oneline -1          # 应为 a1d57cb（或其后）
git status --short            # 应为空
```

- **部署：** 装好的桌面应用，runtime 二进制 = `C:\Users\Administrator\AppData\Local\Zagens\zagens-runtime.exe`。改 runtime 后须 `cargo build --release --bin zagens-runtime` 替换 + 重启 app（或全量打包重装）才在实跑生效。
- **PowerShell 无 heredoc：** 多行 commit message 用「写文件 + `git commit -F .git/ZAGENS_COMMIT_MSG.txt`，提交后删」。
- **行尾：** 仓库在 Windows 报 `LF→CRLF` 是常规提示,忽略。
- **不要**把 `crates/desktop/binaries/*.exe`、`crates/desktop/bundle-legal/*.txt` 提进源码树(暂存时用明确路径,别 `git add -A`)。

---

## 1. 实施顺序（建议；每条独立可提交）

> 编号沿用审计 §0 的跨工具主题 Cn / §5 backlog。**先做快速高价值的 C2、空 search 防呆,再啃进程树。**

### ★ T1 — foreground `exec_shell` 透传 `cwd`（C2，P1，已二次核实，~5 行）
**这是最高性价比的一条,直接消除 MicroStack 那类 `go mod init` 落错目录。**
- **根因（已核实）：** `crates/runtime-server/src/tools/shell/tools/exec.rs:318` 的 foreground 分支调用 `execute_foreground_via_background(context, command, …)`，该函数内部 `manager.execute_with_options_env(command, None, …)` 把 `working_dir` 写死 `None`（`helpers.rs:66-68`）。而 background/interactive 分支（`exec.rs:296`/`308`）都正常传 `working_dir.as_deref()`。
- **改法：** 给 `execute_foreground_via_background` 加一个 `working_dir: Option<&str>` 形参,内部把 `None` 换成它;`exec.rs:318` 调用处传 `working_dir.as_deref()`（`working_dir` 在 `exec.rs:168-179` 已解析好,就在同一作用域）。
- **测试：** `crates/runtime-server/src/tools/shell/tests.rs`。加用例:foreground `exec_shell { cwd: <子目录> }` 后命令在该目录执行（如 `pwd`/`cd` 回显,或写文件验证落点）。
- **顺带（同属 cwd 失效）：** `exec.rs:210` OpenSandbox 分支 `backend.exec(command, &extra_env)` 也没传 `working_dir`——确认 backend 接口能否接收 cwd,能则一并补。

### T2 — `edit_file` 空 `search` 防呆（P0，~3 行）
- **根因：** `crates/runtime-server/src/tools/file/edit.rs:209-264` 不拒绝空 `search`;`replace_mode:"all"` + 空 search 会在每个 UTF-8 边界插入 → **破坏整文件**。
- **改法：** 取到 `search` 后,若 `search.is_empty()`（或 trim 后为空)直接返回 `ToolResult::error`,提示「search 不能为空」。`required_str` 之后加一道校验即可。
- **测试：** `crates/runtime-server/src/tools/file/tests.rs`。加:空 search + `replace_mode:all` → error,文件不变。

### T3 — Windows 杀进程树（C1，P0，本条工作量最大）
- **根因：** `crates/runtime-server/src/tools/shell/process.rs:170-173`(非 unix `kill_child_process_group` 退化为 `child.kill()`)、`process.rs:454-461`(Drop)、`manager.rs:375-376`/`473-474`(超时 kill)、`cancel.rs:99-101`——全部只杀直接子进程,`Start-Process`/守护进程化的孙进程变孤儿继续占端口（今日 7878 即此）。
- **改法（二选一或并用）：**
  - **Job Object**(更稳)：spawn 时把子进程加入一个带 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 的 Windows Job Object,kill 时 close job → 整树终止。需 `windows`/`winapi` crate(查 `Cargo.toml` 现有依赖,优先复用)。
  - **`taskkill /T /F /PID <pid>`**(轻量)：在非 unix 的 kill 路径调用,`/T` 杀进程树。实现快但依赖外部命令。
  - 把三处(timeout kill / cancel / Drop)统一走同一个 `kill_process_tree(pid)` helper。
- **测试：** `crates/runtime-server/src/tools/shell/tests.rs`(Windows 专用 `#[cfg(windows)]`)。启一个会派生子进程的命令,kill 后断言孙进程已终止/端口已释放。注意 CI 跨平台,unix 分支保持现状。
- **参考：** 审计 §0 C1、§1 跨平台段。

### T4 — sync 路径 reader 无界 join（P0）
- **根因：** `process.rs:378-379` + `manager.rs:371-379` sync 路径超时 kill 后对 `read_to_end` 线程**无界 `join()`**,grandchild 持管道时永不 EOF。背景路径已用 `join_reader_bounded`(`process.rs:208-217`)缓解。
- **改法：** sync 路径复用 `join_reader_bounded`(或同等带超时 detach 策略),与背景路径对齐。
- **测试：** `process.rs` 内已有 `join_reader_bounded` 单测(`process.rs:497-525`)可作模板。

### T5 — SSRF：重定向/分支不复校验 IP（C3，P0，安全）
- **根因：** `crates/runtime-server/src/tools/fetch_url.rs:193-214` 初始 URL 校验内网 IP,但 `redirect(Policy::limited(5))` 跟随后不再校验;`fetch_url.rs:182-183` DNS 失败时跳过 SSRF 检查放行;`crates/runtime-server/src/tools/web_run/page.rs:39-89` 完全无 IP 阻断(仅 network policy)。
- **改法：** `fetch_url` 用自定义 redirect policy,对每一跳目标 host 解析后再过 `is_restricted_ip`;DNS 失败应拒绝而非放行;`web_run/page` 取页前补 `is_restricted_ip`/localhost 阻断,与 `fetch_url` 共用一个校验函数。
- **注意：** 这是安全项,改完务必加单测覆盖「公网→302→169.254.169.254 被拦」「DNS 失败被拒」。

### T6 — async 内同步 `Command::output()` 阻塞（C4，P1）
- **位置：** `git.rs:259`、`git_history.rs:447`、`test_runner.rs:86`、`diagnostics.rs:165`、`describe_image.rs:248`(blocking reqwest)。
- **改法：** 改 `tokio::process::Command` + `.output().await`,或包 `tokio::task::spawn_blocking`;`describe_image` 改异步 reqwest。逐文件改,各自有调用点。

### T7 — 其余 P1（可分批）
- **C5 symlink：** `crates/runtime-adapters/src/tools/workspace_walk.rs:27` `follow_links(true)` → 改默认 `false`,或对 walk 出的每个路径再做 workspace 边界校验(grep/glob/file_search/project 共用此 walk,改一处全受益)。
- **C6 子进程/HTTP 无超时：** `test_runner.rs:108`/`office_write.rs:1305`(超时不 kill,留 Python 孤儿) 加 timeout + kill;web 抓取(`fetch_url.rs:223`/`web_run/page.rs:63`)改流式 + `Content-Length` 上限 + cancel token 绑定。
- **C8 编码保留：** `write_file`(`write.rs:184-191`)按读到的编码回写而非静默转 UTF-8;`edit_file`(`edit.rs:157`)/`apply_patch`(`apply_patch.rs:850`)/`fim`(`fim.rs:117`)改用 `detect_and_decode`(读侧已有,复用)。
- **edit_file/fim 原子写：** `edit.rs:272,370,479,558` + `fim.rs:166` 的 `fs::write` 改复用 `write.rs:215-232` 的 `atomic_write`。
- **C7 截断报总数：** `file_search.rs:160-163` 补 `total_matches`/`truncated` 字段(对齐 grep/glob);`shell_output.rs:66-76` 的 summary 从尾部末尾向上扫(现从尾段首部扫,被 cargo `Compiling` 占满配额丢掉 `test result:`)。
- **grep UTF-16：** `search.rs:247-250` 让 `detect_and_decode` 先于 `is_probably_binary` 的 NUL 嗅探(或对 UTF-16 BOM 放行),否则 UTF-16 文本搜不到。
- **grep Windows glob：** `search.rs:583-590,631` 的 include/exclude `matches_glob` 把 `\` 规范为 `/`(`glob_files.rs:42` 已有做法,照搬)。

### T8 — P2（体验/保真,有空再做）
见审计 §5 P2:shell `timeout_ms` 下限对齐 schema、apply_patch fuzz 默认对齐文档、list_dir `offset` 分页、HTML 实体/CJK 换行、describe_image 支持 webp、office 合并单元格内容、`web.run` 的 `screenshot` 名实对齐(实为 PDF 文本拼接)。

---

## 2. 验证命令（每改一项跑）

```powershell
# 编译 + 该 crate 单测
cargo check -p deepseek-runtime-server
cargo test -p deepseek-runtime-server tools::shell      # T1/T3/T4
cargo test -p deepseek-runtime-server tools::file       # T2/T8(edit)
cargo test -p deepseek-runtime-server --lib tools::      # 全工具

# core 侧(若动 loop_guard 等)
cargo test -p deepseek-core

# web-ui(若动前端;一般本批不动)
cd crates/desktop/web-ui; npm run build
```

> 注：`cargo clippy` 全量会在**无关 crate** `deepseek-topic-memory` 报一处 `invalid_regex`(预先存在,见 CHANGELOG 35 行),非本批引入,可忽略或顺手修。

---

## 3. 提交规范

- 沿用仓库风格:`fix(runtime): <scope>` / `feat(runtime): …`。每条 backlog **独立小提交**比一锅烩好 review。
- **每个提交同步：** ①改 `docs/tech/TOOL_SURFACE_AUDIT.md` 对应条目状态为「已缓解」+ 标 commit；②`CHANGELOG.md` `[Unreleased]` 加一条(中文,带 `文件:行号` 与验证结论,沿用现有条目密度)。
- 多行 message:`.git/ZAGENS_COMMIT_MSG.txt` + `git commit -F`,提交后 `del`。

---

## 4. 注意事项 / 陷阱

- **不要全采纳审计每条就改**——T5(SSRF)、T3(进程树)涉及安全与平台 API,改前先确认现有依赖(`windows`/`winapi`/reqwest feature)避免引入新依赖面;按 `.cursor/rules/security-trust.mdc`,新依赖须正常审查。
- **trust_mode 路径越界**(`spec.rs:347`)是**按设计**,不要当 bug 去堵。
- **`test_runner` 外层 success=true / 测试失败**(`test_runner.rs:102`)也是**按设计**(JSON 内 `success:false`),勿改语义。
- **shell 无状态**(跨调用无 cwd 持久)是设计,T1 只修「单次调用内 foreground 丢 cwd」,不是要做 session cwd。
- 改 `workspace_walk`(C5)影响 4 个搜索工具,改完把 grep/glob/file_search/project 的测试都跑一遍。

---

## 5. 相关文档

- [`docs/tech/TOOL_SURFACE_AUDIT.md`](../docs/tech/TOOL_SURFACE_AUDIT.md) — 权威 backlog(§0 共性主题、§1–§4 分工具、§5 优先级)
- `CHANGELOG.md` `[Unreleased]` — 工具面审计 v1 条目 + MicroStack03 复盘修复条目(背景)
- `crates/runtime-server/src/prompts/base.md` — 已含 exec_shell 无状态 / auto-mkdir / 重读 / checklist 纪律的模型侧指引(T1 修好后这些 prompt 兜底仍保留)
- 历史交接同目录:`.claude/lht-stability-9of10-handoff.md`(格式参考)
