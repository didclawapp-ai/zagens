# 工具面审计 — Tool Surface Audit

**状态:** v1（2026-05-31，实证驱动初版）
**触发:** MicroStack03 长程压测复盘——最大弱点暴露在**工具功能本身**（健壮性 / 跨平台 / 边界 / 效率与准确性），而非模型能力。
**方法:** 4 个只读探查代理对 `crates/runtime-server/src/tools/**` 逐文件代码核对，每条带 `文件:行号` 实证；标★者已由本审计二次人工核实。非全采纳代理结论（部分"按设计"项已剔除或标注）。
**严重度:** **P0** = 挂死 / 丢数据 / 安全穿透；**P1** = 长任务里高频踩、误判、烧 token；**P2** = 体验 / 保真度。

> 本文件是 **backlog + 现状快照**，不是已修清单。已修项标注「已缓解」。修复时请回填状态并记 CHANGELOG。

---

## 0. 跨工具共性主题（最高价值，优先于单工具修）

| # | 主题 | 维度 | 严重度 | 代表位置 | 现状 |
|---|------|------|--------|----------|------|
| C1 | **Windows 杀不掉进程树** — `child.kill()` 只杀直接子进程，`Start-Process`/守护进程化的孙进程变孤儿、继续占端口（今日 7878 占用即此） | 健壮性/跨平台 | **P0** | `process.rs:170-173,454-461`、`manager.rs:375-376,473-474`、`cancel.rs` | **已缓解** — 新增 `kill_process_tree(pid)` 走 `taskkill /T /F /PID`;非 unix `kill_child_process_group` 先树杀再 reap;`ShellChild::kill` 两平台统一走 group/tree kill(Drop / `BackgroundShell::kill` / cancel 自动受益);manager 两处 sync 超时 kill 统一。`#[cfg(windows)]` 单测 `test_exec_shell_kill_terminates_grandchild_process_tree` 验证孙进程被终止。**Job Object 更彻底但需改 spawn 多路;taskkill 为轻量足够方案** |
| C2 | **foreground `exec_shell` 丢弃 `cwd` 参数** ★已核实 — 默认前台路径硬传 `None`，回退工作区根；background/interactive 正常 | 健壮性/边界 | **P1** | `exec.rs:318` → `helpers.rs:66-68`（对比 `exec.rs:296/308` 传 `working_dir`） | **已缓解** — `execute_foreground_via_background` 加 `working_dir` 形参,`exec.rs` 调用处透传 `working_dir.as_deref()`;单测 `test_exec_shell_foreground_respects_cwd`。**残留：** OpenSandbox `backend.exec`(`exec.rs:210`)仍不带 cwd(trait 协议改动,另议) |
| C3 | **SSRF：重定向/分支不复校验 IP** — `fetch_url` 初始 URL 校验内网 IP，但 302 跟随后不再校验；`web_run` 取页仅查 network policy、完全无 IP 阻断 | 健壮性/安全 | **P0**（取决于 network policy 是否默认 allow） | `fetch_url.rs:193-214`、`web_run/page.rs:39-89` | **已缓解** — 新增共享 `tools/ssrf.rs`:`fetch_with_ssrf_guard` 手动跟随重定向(`Policy::none()`),**每跳** host 都过 policy + `is_restricted_ip` + pin 校验后 IP;DNS 失败/零地址**fail closed**;`fetch_url` 与 `web_run/page` 共用。单测:metadata IP / 私网 / loopback / `::1` / localhost / DNS 失败 / 公网 IP 放行 ×5 |
| C4 | **async 里同步阻塞 `Command::output()`** — 阻塞 tokio worker，并发工具相互拖累 | 健壮性/效率 | **P1** | `git.rs:259`、`git_history.rs:447`、`test_runner.rs:86`、`diagnostics.rs:165`、`describe_image.rs:248`(blocking reqwest) | **已缓解** — `git`/`git_history`/`test_runner` 改 `tokio::process::Command` + `.output().await`;`diagnostics` 把全部探测包进 `spawn_blocking`(深层 sync 助手树不动);`describe_image` 改异步 `reqwest::Client` |
| C5 | **`follow_links(true)` 可跟符号链接读出工作区外** — walk 到的文件不过 `resolve_path` | 健壮性/安全 | **P1** | `runtime-adapters/.../workspace_walk.rs:27`（grep/glob/file_search/project 共用） | **已缓解** — 改 `follow_links(false)`(亦对齐 ripgrep 默认),工作区内指向区外的 symlink 不再被跟随;grep/glob/file_search/project 全过 |
| C6 | **子进程/HTTP 无超时 + 响应体全量读** — git/test/office Python/web 抓取无 timeout；web 响应先 `bytes().await` 全读再按上限截断（OOM 风险在截断之前） | 健壮性/边界 | **P1** | `test_runner.rs:108`、`office_write.rs:1305`、`fetch_url.rs:223`、`web_run/page.rs:63` | 部分（输出截断有，但内存/挂起未防） |
| C7 | **静默截断、不报总数** — 结果超上限直接 `truncate`，部分工具无 `total`/`truncated`，模型以为已全 | 准确性 | **P1** | `file_search.rs`（无 total）；`shell_output.rs` 80 行 summary 丢尾部 `test result:` | 部分缓解 — grep/glob 有 `truncated`;`file_search` 现返回 `{matches,total_matches,returned,truncated}`★;`shell_output` summary 待办 |
| C8 | **编码：写侧不保留、edit/patch 仅 UTF-8** — `read_file`/`grep` 已 `detect_and_decode`，但 `write_file` 把 GB18030 静默转 UTF-8；`edit_file`/`apply_patch`/`fim` 仅 `read_to_string`（非 UTF-8 直接报错） | 健壮性/准确性 | **P1** | `write.rs:184-191`、`edit.rs:157`、`apply_patch.rs:850`、`fim.rs:117` | 读侧已缓解；写/改侧未 |

---

## 1. Shell 类（`exec_shell` / `task_shell_*` / cancel）

**健壮性**
- **[已缓解★] `manager.rs` sync 路径** — 新增 `join_reader_thread_bounded`(返回 `Vec<u8>` 版的有界 join);sync 路径**成功与超时两条**出口的 stdout/stderr `join()` 均改用它,grandchild 持管道时按 `READER_DRAIN_GRACE` detach 而非无界阻塞。新增两单测(detach 返回空 buf / 正常返回 buffer)。
- **[已缓解★] C1**（见上）— Windows 进程树 kill（`taskkill /T /F`）。
- **[P1] `process.rs:175-187`** — detach 后的 reader 仍向 `Vec<u8>` 无上限追加 → 长跑 server 日志内存持续涨。
- **[P2] `process.rs:185-189`** — `buffer.lock()` 失败 `break` 静默丢输出。

**跨平台**
- **[已缓解★] C1** — `process.rs` 非 unix `kill_child_process_group` 现走 `taskkill /T /F`(树杀);Drop / `BackgroundShell::kill` / cancel 全经 `ShellChild::kill` 统一受益。
- **[P2] `sandbox/mod.rs:142-144`** — 非 Windows 用 `sh -c` 非 bash，`[[`/`source` 等 bashism 失败。
- **[P2] `manager.rs:568-586`** — background spawn 未装 `install_parent_death_signal`（sync/interactive 有），Linux runtime 被 SIGKILL 时可能留孤儿。

**边界**
- **[已缓解★] C2** — foreground 丢 `cwd`（`execute_foreground_via_background` 已透传 `working_dir`）。
- **[P1] `exec.rs:210`** — OpenSandbox 分支 `backend.exec` 不传 `working_dir`，外部 sandbox 下 cwd 无效。
- **[P1] `manager.rs:188-189,283`** — `cwd` 仅校验 workspace 边界，**不校验目录存在**，指向已删目录行为因 OS 而异。
- **[P2] `exec.rs:87` vs `manager.rs:191`** — `timeout_ms` 工具层只 `.min(600k)` 未设下限，manager 层 `clamp(1000,..)`，传 500 实际 1000，和 schema 不符。
- **[设计] `manager.rs:188` + `base.md`** — 跨调用无 cwd 状态（无状态 shell）；prompt 已说明用 `cwd`/`cd &&`。

**效率与准确性**
- **[P1] `shell_output.rs:33-48,66-76`** — 超 30KB 保留 head + 从尾段**首部**扫 ≤80 行 summary；cargo 海量 `Compiling` 占满配额 → 尾部 `test result:` 被丢。**建议从尾部末尾向上扫**。
- **[P1] `large_output_router.rs:240-246` + `registry.rs:190-197`** — 大输出 synthesis 实际只用前 1200 字符，非完整内容。
- **[P2] `large_output_router.rs:197-200`** — `estimate_tokens` 用 chars/3，CJK 低估、路由偏晚。
- **[P2] `manager.rs:713-722`** — `get_output_delta(wait=true)` 持 `shell` 可变引用时 `sleep(100ms)`，阻塞同 session 其它操作（前台用 `wait=false` 影响有限）。

---

## 2. 文件读写/编辑类（`read_file`/`write_file`/`edit_file`/`apply_patch`/`list_dir`/`file_info`/`fim`）

**健壮性**
- **[已缓解★] `edit.rs:149-158`** — 空/纯空白 `search` 现直接 `invalid_input` 报错(导向 `insert_after`/`replace_line`),不再在每个 UTF-8 边界插入破坏整文件;单测 `test_edit_file_empty_search_rejected`(空/空格/`\n\t` 三态 + 文件不变)。
- **[已缓解★] `edit.rs` ×4 + `fim.rs:166`** — `edit_file`/`fim` 改用 `atomic_write`(temp + rename),与 `write_file`/`apply_patch` 一致,崩溃/磁盘满不再留截断文件。
- **[P1] C8** — `edit_file`/`apply_patch`/`fim` 仅 UTF-8，GB18030 文件无法改。
- **[P1] 全写路径** — 无文件锁/版本检查;并行 `edit_file` 同文件后写覆盖先写（`write_file` `supports_parallel=false` 但 runtime 不强制串行）。
- **[P1] `edit.rs:157` / `apply_patch.rs:633` / `read.rs:475-541`(Office) / `fim.rs:117`** — 无文件大小上限，大文件 OOM（普通文本 `read_file` 有 `MAX_FILE_SIZE`，这些路径绕过）。
- **[P1] `apply_patch.rs:837-847`** — rollback 错误被 `let _ =` 吞，主写失败 + rollback 失败 → 半新半旧无上报。
- **[P1] `file_info.rs:56`** — 不区分文件/目录，对目录 `is_text` 可能误判 true。

**跨平台**
- **[已缓解★] `write.rs:107-116,215-232` + `apply_patch.rs:738-781`** — 原子写 + CRLF 保留（有单测）。
- **[P2] 末尾换行不一致** — `apply_patch` 保留 trailing newline；`write_file`（`write.rs:112`）与 `edit_file` 行操作（`edit.rs:346-368`）会丢掉原文件末尾 `\n`。
- **[P2] `read.rs:151`** — 返回给模型统一 LF（行内 `\r` 已 trim），与磁盘 CRLF 字节不一致;metadata 无 `line_ending` 字段。

**边界**
- **[P1] `edit.rs:209`** — 精确子串匹配，无锚定/空白容错 → gofmt 后预写 search 必失配。已加:零匹配提示(`edit.rs:216-222`)、多匹配强制 `replace_mode`(`236-257`)、别名提示(★本轮)、prompt 重读指引(★本轮)；**匹配本身仍精确**。
- **[P2] `apply_patch.rs:212 vs 185`** — schema 写 fuzz 默认 3，代码实际 `MAX_FUZZ=50`，未传时搜索窗 ±50 行，重复上下文易误匹配（文档与实现不符）。
- **[P2] `edit.rs:445`** — `delete_lines` 的 `end_line` 超文件静默 `min(len)`，"删 100-200" 实删到 50 仍报成功。
- **[设计/已缓解] 路径越界** — 非 trust 模式统一 `resolve_path`(canonicalize + no `..`);`trust_mode=true` 按设计跳过（`spec.rs:347`）。Windows 保留名(CON/NUL)/长路径无专门处理。

**效率与准确性**
- **[已缓解★] `read.rs:90-99`** — `offset`/`limit` 分页 + 流式读。
- **[P2] `edit.rs:284`** — 每次成功都全文件 `make_unified_diff`，大文件无 `DIFF_MAX_INPUT_BYTES` 跳过（`write.rs:146` 有）。
- **[P2] `list_dir.rs:57-83`** — 仅 `limit` 无 `offset`/cursor，超大目录无法翻页（有 `truncated`）。

---

## 3. 搜索/检索类（`grep_files`/`glob_files`/`file_search`/`project_*`/`git_*`）

**健壮性**
- **[已缓解★] `workspace_walk.rs is_probably_binary`** — 现对 UTF-16 LE/BE BOM 与 UTF-8 BOM 放行(不当二进制),NUL 启发式仅作用于无 BOM 文件 → 带 BOM 的 UTF-16 文本可被 grep 搜到。
- **[P0/P1] `search.rs:215-262`** — 无超时/无 `spawn_blocking`，先全量 `collect_files` 再逐文件 `fs::read`（≤10MB/文件）+ `lines().collect()`;大 monorepo 阻塞 async + 大内存。自实现 walk，**非 ripgrep 进程**。
- **[P1] `search.rs:255-257`** — `fs::read` 失败与二进制嗅探共用 `files_skipped_binary`，权限/IO 错误被误报为"跳过二进制"→ 静默漏搜。
- **[P1] C5** — `follow_links(true)` 跟符号链接出工作区。

**跨平台**
- **[已缓解★] `search.rs:583-591`** — `grep_files` include/exclude 匹配前把相对路径 `\` 规范为 `/`,Windows 下 `src/**/*.rs` 正常匹配。
- **[P2] `search.rs:262,270`** — 解码后只按 `\n` 分行，CRLF 残留 `\r`，正则可能不匹配 `fn foo`(实为 `fn foo\r`)。
- **[P2] `git.rs:72,168`** — pathspec 用 `display()`，Windows 反斜杠。

**边界**
- **[已缓解★] `file_search.rs`** — 不再返回裸数组,改为 `{matches,total_matches,returned,truncated}`;超 `limit` 时 `truncated=true` 且 `total_matches` 报全量,模型可知结果被截断（C7）。
- **[P1] `glob_files.rs:60-61,116-123`** — schema 说 pattern「relative to path」,实际按 **workspace 相对**匹配 → `path:"src"` + `*.ts` 常不匹配(需 `**/*.ts`)。
- **[P1] `file_search.rs:128`** — 固定尊重 gitignore，无 `respect_gitignore` 参数（grep/glob 有），无法搜被 ignore 的文件。
- **[P1] `project.rs:52-75`** — `project_tree` 无输出上限、三次独立 walk;宽目录撑爆上下文。
- **[P2] `search.rs:157-158`** — `context_lines` 解析失败用 `usize::MAX`，可构造超大上下文。
- **[P2] `glob_files.rs:109-112`** — `base_path` 不存在返回空 + success（grep 有"路径不存在"错误）。

**效率与准确性**
- **[已缓解★] `search.rs:277-280`** — `files_with_matches` 单文件首命中 `break`；**但 `count` 模式仍全文件扫**(`270-275`)。
- **[已缓解★] `search.rs:755-760`** — BM25 用 `file_match_total` 预计算去掉 O(文件×匹配)。
- **[P1] `search.rs:328`** — 每次 grep 调 `ensure_symbol_index`,陈旧时后台全量构建。
- **[P2] `search.rs:318-321`** — BM25 仅对**已截断**的 matches 重排,第 101 个相关命中(在未扫文件)模型永远看不到（有 `truncated`）。
- **[部分缓解] `git.rs`/`git_history.rs` C4 + C6** — C4 阻塞 async 已修(tokio::process);**C6 子进程无超时仍未做**;大 diff 先挂起再 40k 截断。

---

## 4. Web/网络 + 实用工具（`web_search`/`fetch_url`/`web.run`/`validate_data`/`diagnostics`/`test_runner`/`describe_image`/`write_office`）

**健壮性 / 安全**
- **[已缓解★] C3** — 共享 `tools/ssrf.rs::fetch_with_ssrf_guard` 手动跟随重定向、每跳复校验 IP;`fetch_url` 与 `web_run/page` 共用。
- **[已缓解★] `fetch_url` DNS 失败** — `validate_url_ssrf` 现 DNS 失败/零地址 **fail closed**(拒绝),不再放行。
- **[P1] C6** — `web_search.rs:248`/`page.rs:63`/`fetch_url.rs:223` 全量读响应体(只有展示截断,无内存/带宽防护);无 `CancellationToken` 绑定,取消后请求仍跑满 timeout。
- **[P1] `web_run/search.rs:13-98`** — `web.run` 的 search 不调 `check_host_policy`、DDG 失败不 fallback Bing → 与 `web_search` 策略/结果不一致(policy deny 时仍可能出网)。
- **[P1] `test_runner.rs:108`/`office_write.rs:1305`** — 无 timeout / 超时不 `kill` → Python 孤儿 + 文件锁残留。
- **[已缓解★] C4** — `describe_image` 改异步 `reqwest::Client`;`diagnostics` 探测包进 `spawn_blocking`;`test_runner` 改 `tokio::process`。

**跨平台**
- **[P1] `office_write.rs:1246,1050`** — docx/pptx/pdf 依赖 `resolve_python_for_office()`,Windows 干净环境首次需 PATH Python + 联网 pip 建 venv 易失败(xlsx 纯 Rust 不受影响)。
- **[P2] `describe_image.rs:232`** — 默认 `base_url` 硅基流动,海外/无该服务时默认失败(可 config 覆盖)。

**边界**
- **[P2] `fetch_url.rs:235-244`/`web_run/page.rs:222-246`** — 非 HTML(PDF/二进制)仍 `from_utf8_lossy` 出乱码无类型警告;`web.run` 的 `screenshot` 实际只返回 PDF 文本拼接,**非图像**(名实不符)。
- **[P2] `web_search.rs:307-313`** — HTTP 200 零结果仍 success,难区分真无结果 vs HTML 改版。
- **[P1] `validate_data.rs:118`** — `read_to_string` 无文件大小上限。
- **[P2] `describe_image.rs:118`** — 仅 png/jpg/jpeg/gif/bmp,不含 webp/heic(常见截图)。

**效率与准确性**
- **[P2] `fetch_url.rs:289-297`/`web_run/html.rs:204-212`** — HTML 实体解码仅 6-7 种字面量,无 `&#...;`/`&#x...;`;markdown 保真度低。
- **[P2] `web_run/html.rs:181-192`** — `wrap_line` 按 UTF-8 **字节**算宽,CJK 单行远超 `wrap_width`。
- **[P2] `web_run/types.rs:142` / `search.rs:298`** — `FindResult.count`/图片过滤为截断后数,非总命中。
- **[P2] `office_write.rs:785`** — 合并单元格只写 `""`,内容丢失。

---

## 5. 优先级 backlog（建议修复顺序）

### P0（挂死 / 丢数据 / 安全）
1. **C1 Windows 进程树 kill** — ✅ 已缓解(`taskkill /T /F /PID`,kill/cancel/Drop/manager 超时四路统一)。Job Object 为后续更彻底选项。
2. **C3 SSRF** — ✅ 已缓解(共享 `tools/ssrf.rs`,每跳复校验 + DNS fail closed)。
3. **edit_file 空 search 防呆** — ✅ 已缓解(`edit.rs:149-158`,空/纯空白直接报错)。
4. **grep UTF-16** — ✅ 已缓解(`is_probably_binary` 对 UTF-16/UTF-8 BOM 放行)。
5. **sync 路径 reader 无界 join** — ✅ 已缓解(`join_reader_thread_bounded`,sync 成功/超时两路均有界)。

### P1（长任务高频痛点 — 多为快速修）
6. **C2 foreground 透传 `cwd`** ★ — `execute_foreground_via_background` 加 `working_dir` 参数,`exec.rs:318` 传 `working_dir.as_deref()`。**~5 行,直接消除 MicroStack 那类 cwd 落错**。
7. **C4 async 阻塞** — ✅ 已缓解(git/git_history/test_runner→tokio::process;diagnostics→spawn_blocking;describe_image→异步 reqwest)。
8. **C6 子进程/HTTP 超时 + kill** — test_runner/office Python 加 timeout 并 kill;web 抓取流式 + `Content-Length` 上限 + cancel 绑定。
9. **C5 symlink** — ✅ 已缓解(`workspace_walk` 改 `follow_links(false)`)。
10. **C7 截断报总数** — ✅ `file_search` 补 `total_matches`/`truncated`(已缓解);`shell_output` summary 从尾部扫(待办)。
11. **C8 编码保留** — `write_file` 按读到的编码回写;`edit_file`/`apply_patch`/`fim` 改 `detect_and_decode`。
12. **edit_file/fim 原子写** — ✅ 已缓解(复用 `atomic_write`)。
13. **glob_files/file_search 语义** — glob 相对基准对齐 schema;`file_search` 加 `respect_gitignore`。

### P2（体验 / 保真度）
- shell `timeout_ms` 下限对齐 schema;apply_patch fuzz 默认对齐文档;list_dir `offset` 分页;HTML 实体/CJK 换行;describe_image 支持 webp;office 合并单元格内容;`screenshot` 名实对齐。

---

## 6. 后续
- 本审计是 **v1 快照**。建议:每修一项回填「已缓解」+ 记 CHANGELOG;每次大压测后把新暴露的工具问题追加进 §0–§4。
- 与 `docs/harness/` 的 LHT 测试用例联动:压测复盘里凡是"工具手感"类问题,先在这里登记再决定是否修。
- 修复验证优先复用既有单测目录(`tools/shell/tests.rs`、`tools/file/tests.rs`、`search.rs` 内联测试)。
