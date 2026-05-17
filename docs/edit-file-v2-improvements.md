# edit_file 改进方案 V2

**日期**: 2026-05-17  
**基于**: V1已实施版本（E1-E5全部落地）+ 右键菜单实施会话（JSX污染事故）  
**原则**: 不改API签名，向后兼容，轻量  

---

## V1 已实施状态回顾

| 项 | 内容 | 状态 |
|----|------|:----:|
| E1 | CRLF自适应换行符 | ✅ file.rs:1161-1173 |
| E2 | start_line/end_line限定搜索范围 | ✅ file.rs:1177-1192 |
| E3 | 诊断性错误信息（NOT_FOUND含HINT） | ✅ file.rs:1196-1218 |
| E4 | count>1时[AMBIGUOUS]警告+行号 | ✅ file.rs:1223-1243 |
| E5 | 替换成功返回命中行号 | ✅ file.rs:1264-1273 |

V1效果验证：main.rs（CRLF）多行编辑从15次调用/6次失败→4次调用/0次失败。

---

## V2 新增问题诊断

来源：右键菜单实施会话（68次工具调用，JSX文件污染事故）

| # | 根因 | 证据 | 影响 |
|---|------|------|------|
| R6 | 所有操作成功返回中无`total_lines` | 模型在insert_after后不知道文件新总行数，后续delete_lines行号盲猜 | delete_lines删多/删少，需额外修补 |
| R7 | delete_lines无预览，盲删 | delete_lines(840,885)删掉了`</ul>`和`}`等JSX结构——超出预期范围 | 需额外read_file确认+回滚 |
| R8 | JSX/TSX文件无语法预校验 | `{condition && (<ul>...</ul>{ctxMenu && (...)})}` 缺少Fragment包裹——插入前无法感知，tsc报错才发现 | 2-3轮额外修复回合 |
| R9 | exec_shell在Windows cmd.exe下PowerShell语法失败 | 22次exec_shell全部失败（Set-Location/python/Out-File均不识别） | 5次无效旁路尝试（Python/PowerShell） |

---

## V2 改动（4项，~90行）

---

### E6：所有写操作成功返回中附加 total_lines（P0）

**问题**：`insert_after`/`delete_lines`/`replace_line`/`search_replace`成功后，返回信息里只有diff和summary，没有操作后的文件总行数。模型在第N次insert后行号偏移，后续操作全靠猜测。

**现状**（file.rs:1363）：
```rust
let summary = format!("Inserted {inserted_count} line(s) at {position} in {display}");
```

**改动**：在每个操作的summary行末尾附加`total_lines`，4处修改，全部在summary格式化字符串处：

**search_replace**（file.rs:1271-1273）：
```rust
let total_lines = updated.lines().count();
let summary = format!(
    "Replaced {count} occurrence(s) in {display} ({}) — file now {total_lines} lines",
    line_list.join(", ")
);
```

**insert_after**（file.rs:1363）：
```rust
let total_lines = updated.lines().count();
let summary = format!(
    "Inserted {inserted_count} line(s) at {position} in {display} — file now {total_lines} lines"
);
```

**delete_lines**（file.rs:1442）：
```rust
let total_lines = updated.lines().count();
let summary = format!(
    "Deleted {deleted_count} line(s) ({range}) in {display} — file now {total_lines} lines"
);
```

**replace_line**（file.rs:1517）：
```rust
let total_lines = updated.lines().count();
let summary = format!(
    "Replaced line {line} in {display} — file now {total_lines} lines"
);
```

`updated`在每个操作里已经计算好，`updated.lines().count()`是纯内存操作，零IO开销。

**改动量**：file.rs 4处summary行 +4行（各加一行`total_lines`计算）  
**效果**：模型做完insert_after后立刻知道"文件现在是X行"，后续delete_lines/read_file的行号参数准确  
**验收**：任意edit_file操作成功 → 返回信息包含`file now N lines`

---

### E7：delete_lines 执行前返回被删内容预览（P0）

**问题**：delete_lines是破坏性操作，执行前没有"将删除哪些内容"的预览。行号偏移后盲删很容易超出预期范围——本次事故中删掉了`</ul>`和`}`结构，造成JSX文件污染。

**方案**：新增可选参数`dry_run: bool`，默认false。当`dry_run: true`时，只返回将被删除的行内容，不写盘：

**schema新增**：
```rust
"dry_run": {
    "type": "boolean",
    "description": "If true, preview what would be deleted without modifying the file. Returns the lines that would be removed."
}
```

**execute_delete_lines逻辑**（在fs::write之前插入）：
```rust
let dry_run = optional_bool(input, "dry_run", false);

// 计算将被删除的行
let deleted_lines: Vec<&str> = lines[start.saturating_sub(1)..e].to_vec();
let deleted_preview = deleted_lines
    .iter()
    .enumerate()
    .map(|(i, l)| format!("  [{:>4}] {}", start + i, l))
    .collect::<Vec<_>>()
    .join("\n");

if dry_run {
    return Ok(ToolResult::success(format!(
        "[DRY_RUN] Would delete {deleted_count} line(s) ({range}) in {display}:\n{deleted_preview}\n\
        To confirm, call delete_lines again without dry_run: true."
    )));
}

// 原有的fs::write逻辑不变
fs::write(&file_path, &updated)...
```

**改动量**：file.rs `execute_delete_lines` +~15行  
**效果**：模型在delete_lines前先dry_run确认内容，避免盲删结构性代码（JSX的`</ul>`、Rust的`}`等）  
**验收**：`delete_lines(dry_run: true, start_line: 840, end_line: 885)` → 返回840-885行内容预览，不修改文件

---

### E8：TSX/JSX文件写操作后附加语法校验（P1）

**问题**：JSX/TSX文件里插入代码后，如果新代码缺少`Fragment`包裹、括号不匹配等，只有`tsc`运行后才能发现。本次事故中，正确的JSX需要`<></>`包裹，但模型在插入前没有感知到这个约束，导致2-3轮额外修复。

**方案**：对`.tsx`/`.jsx`文件，在写盘成功后做轻量语法检查——不依赖`tsc`（太重），只做括号/标签平衡检查：

新增辅助函数`check_jsx_balance()`：

```rust
/// Lightweight JSX balance check for .tsx/.jsx files.
/// Checks: brace balance, JSX tag balance, Fragment consistency.
/// Returns a warning string if imbalance detected, None if looks OK.
fn check_jsx_balance(content: &str) -> Option<String> {
    let mut brace_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut warnings = Vec::new();

    for ch in content.chars() {
        if in_string {
            if ch == string_char { in_string = false; }
            continue;
        }
        match ch {
            '"' | '\'' | '`' => { in_string = true; string_char = ch; }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    warnings.push("unmatched closing brace '}'".to_string());
                    brace_depth = 0;
                }
            }
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    warnings.push("unmatched closing paren ')'".to_string());
                    paren_depth = 0;
                }
            }
            _ => {}
        }
    }

    if brace_depth != 0 {
        warnings.push(format!("unbalanced braces: {} unclosed '{{'", brace_depth.abs()));
    }
    if paren_depth != 0 {
        warnings.push(format!("unbalanced parens: {} unclosed '('", paren_depth.abs()));
    }

    if warnings.is_empty() { None } else { Some(warnings.join("; ")) }
}
```

**调用位置**：在每个写操作的`fs::write`成功后、生成`summary`前，对`.tsx`/`.jsx`文件调用：

```rust
// 在 fs::write 成功后插入（search_replace/insert_after/delete_lines/replace_line 共4处）
let jsx_warning = if matches!(
    file_path.extension().and_then(|e| e.to_str()),
    Some("tsx") | Some("jsx")
) {
    check_jsx_balance(&updated)
        .map(|w| format!("\n[JSX_WARNING] {w} — run tsc to verify"))
        .unwrap_or_default()
} else {
    String::new()
};

// 在 full_body 末尾附加
let full_body = format!("{existing_body}{jsx_warning}");
```

**改动量**：file.rs 新增`check_jsx_balance()`~30行 + 4处调用点各+5行 = ~50行  
**效果**：模型在修改TSX文件后立即收到括号不平衡警告，不需要等tsc才发现错误  
**限制**：这是轻量启发式检查，不能检测JSX Fragment缺失（需要真正的AST解析），但能捕获本次事故中最核心的`{`/`}`/`(`/`)`不平衡  
**验收**：向TSX文件插入缺少`)`的代码块 → 返回`[JSX_WARNING] unbalanced parens`；平衡的代码 → 无警告

---

### E9：exec_shell Windows自动探测PowerShell（P0）

**问题**：Windows环境下，`exec_shell`通过`cmd.exe`执行命令。模型发出PowerShell语法（`Set-Location`、`Out-File`、管道`|`等）时，`cmd.exe`全部不识别，22次失败，5次无效旁路。`CommandSpec::shell()`在Windows下硬编码走`cmd /C`，没有探测PowerShell可用性。

**现状**（sandbox.rs里的`CommandSpec::shell()`——从shell.rs调用链确认）：
Windows下命令构建走`cmd /C`固定路径，无fallback。

**方案**：在`ShellManager`初始化时做一次PowerShell探测，缓存结果；后续命令构建优先用PowerShell：

在`shell.rs`的`ShellManager`里新增字段和初始化逻辑：

```rust
pub struct ShellManager {
    // ... 现有字段 ...
    /// Cached Windows shell preference: true = PowerShell available and preferred.
    #[cfg(windows)]
    prefer_powershell: bool,
}

impl ShellManager {
    pub fn new(workspace: PathBuf) -> Self {
        #[cfg(windows)]
        let prefer_powershell = detect_powershell();

        Self {
            // ... 现有字段 ...
            #[cfg(windows)]
            prefer_powershell,
        }
    }
}

/// Probe for PowerShell availability on Windows (runs once at startup).
#[cfg(windows)]
fn detect_powershell() -> bool {
    // Try pwsh (PowerShell 7+) first, then powershell (Windows PowerShell 5.1)
    for ps in &["pwsh", "powershell"] {
        if std::process::Command::new(ps)
            .args(["-NoProfile", "-NonInteractive", "-Command", "exit 0"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}
```

**在`execute_sync_sandboxed`和`spawn_background_sandboxed`中**，Windows下根据`prefer_powershell`选择shell：

实际上`CommandSpec::shell()`在`crates/tui/src/sandbox.rs`里——改动入口是那里，传入`prefer_powershell`标志：

```rust
// sandbox.rs CommandSpec::shell() Windows分支
#[cfg(windows)]
pub fn shell_with_ps(command: &str, work_dir: PathBuf, timeout: Duration, prefer_ps: bool) -> Self {
    if prefer_ps {
        // 优先pwsh，fallback powershell
        let ps_exe = if which_powershell() == "pwsh" { "pwsh" } else { "powershell" };
        Self {
            program: ps_exe.to_string(),
            args: vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                command.to_string(),
            ],
            // ...
        }
    } else {
        // 现有cmd /C路径不变
        Self { ... }
    }
}
```

**改动量**：
| 文件 | 改动量 |
|------|--------|
| `shell.rs` | `ShellManager`加`prefer_powershell`字段 + `detect_powershell()`函数 +~20行 |
| `sandbox.rs` | `CommandSpec::shell()`加PowerShell分支 +~15行 |

**效果**：消除100%的Windows PowerShell语法失败（22次失败→0次）  
**验收**：Windows环境下`exec_shell("Get-ChildItem .")` → 正常返回目录列表，不再报`not recognized`

---

## 实施顺序

| 批次 | 项 | 改动量 | 收益 |
|:----:|-----|--------|------|
| **M** | E6（total_lines，4处）+ E9（PowerShell探测） | ~24行 | 立即消除行号盲猜+PowerShell失败两大问题 |
| **N** | E7（delete_lines dry_run）+ E8（JSX平衡检查） | ~65行 | 消除盲删和JSX污染 |

---

## 改动汇总

| 项 | 文件 | 净增行数 |
|----|------|---------|
| E6 total_lines | `file.rs` | +8行（4处各+2行） |
| E7 delete dry_run | `file.rs` | +15行 |
| E8 JSX balance check | `file.rs` | +50行 |
| E9 PowerShell探测 | `shell.rs` + `sandbox.rs` | +35行 |
| **总计** | | **~108行** |

---

## 改动前后对比

| 场景 | V1后 | V2后 |
|------|------|------|
| insert_after后续行号 | 需read_file确认 | 返回中直接含`file now N lines` |
| delete_lines盲删 | 无预警，需回滚 | dry_run先预览再确认 |
| TSX括号错误 | 等tsc报错才发现 | 写盘后立即[JSX_WARNING] |
| Windows PowerShell命令 | 22次失败，旁路5次 | 自动探测，优先PowerShell |

---

## 注意事项

**E8的局限**：轻量括号检查无法替代真正的TypeScript编译器。它能捕获`{`/`}`/`(`/`)`不平衡，但无法检测：
- JSX Fragment缺失（`<ul>`和`{expr}`并列需要`<></>`包裹）
- 类型错误
- import缺失

完整的JSX语法验证需要`tsc --noEmit`，但那是重量型操作（秒级）。E8的定位是"快速拦截最常见的结构性错误"，不是"替代tsc"。本次事故的根本原因（Fragment缺失）E8还是拦不住，但`tsc`集成到dry_run流程是V3的候选方向。

**E9的fallback**：`detect_powershell()`在启动时运行一次，结果缓存在`ShellManager`。如果PowerShell不可用（极少数Win10+环境），自动fallback到现有`cmd /C`路径，不影响现有行为。
