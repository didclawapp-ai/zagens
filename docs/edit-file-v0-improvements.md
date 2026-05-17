# edit_file 改进方案

**日期**: 2026-05-17  
**基于**: `file.rs`（1699行）+ V5实施会话（68次工具调用记录）  
**原则**: 不改API签名，向后兼容，轻量  

---

## 问题诊断

V5实施会话中，68次工具调用的时间分布：

| 类别 | 次数 | 浪费程度 |
|------|------|---------|
| edit_file 成功 | 25次 | — |
| edit_file 失败 | 6次 | 🔴 每次失败触发一轮重试循环 |
| 无效旁路尝试（Python/PowerShell） | 5次 | 🔴 100%浪费，零产出 |
| 多余的预读/验证 read_file | 12次 | 🟡 部分可消除 |

**根因一览**：

| # | 根因 | 证据 | 发生位置 |
|---|------|------|---------|
| R1 | `\r\n` vs `\n` 换行符不一致 | main.rs（Windows换行）6次失败；symbol_index.rs（Unix换行）20+次零失败 | V5-1 |
| R2 | 搜索字符串在文件中多处命中导致误伤 | `.unwrap_or_else` 匹配了line 3468和3849两处 | V5-1 |
| R3 | 失败时错误信息只说"not found"，不说原因 | 模型转而尝试Python/PowerShell旁路，5次全失败 | V5-1 |
| R4 | struct加字段后需逐轮编译才能发现所有缺失 | V5-7/V5-8中2轮编译修复，6次额外edit_file | V5-7/V5-8 |
| R5 | count > 1时仍全部替换，无法控制替换哪一处 | 现有代码：`contents.replace(search, replace)` 替换所有命中 | file.rs:1064 |

---

## 改进方案（5项，~80行）

---

### E1：换行符自适应匹配（P0，最高优先级）

**问题**：`fs::read_to_string` 在Windows上读到的内容含`\r\n`，但模型通过`read_file`看到的是流式逐行输出（`\n`拼接），构造的search字符串用`\n`——两者不同，多行搜索必然失败。

**现有代码**（file.rs:1039-1064）：
```rust
let contents = fs::read_to_string(&file_path)...?;
let count = contents.matches(search).count();
if count == 0 {
    return Err(ToolError::execution_failed(format!(
        "Search string not found in {}",
        file_path.display()
    )));
}
let updated = contents.replace(search, replace);
```

**修复方案**：在匹配之前，检测文件实际换行符，将search/replace中的`\n`统一转换为文件实际使用的换行符：

```rust
let contents = fs::read_to_string(&file_path)...?;

// E1: 自适应换行符 — 检测文件实际换行符，normalize search/replace
let file_line_ending = if contents.contains("\r\n") { "\r\n" } else { "\n" };
let search_normalized = if file_line_ending == "\r\n" {
    // search来自模型（\n），文件是\r\n → 转换
    search.replace("\r\n", "\n").replace('\n', "\r\n")
} else {
    search.to_string()
};
let replace_normalized = if file_line_ending == "\r\n" {
    replace.replace("\r\n", "\n").replace('\n', "\r\n")
} else {
    replace.to_string()
};

let count = contents.matches(search_normalized.as_str()).count();
if count == 0 {
    // → E3的改进错误信息
}
let updated = contents.replace(search_normalized.as_str(), &replace_normalized);
```

**改动量**：file.rs `EditFileTool::execute` +~15行  
**效果**：消除V5-1中100%的换行符失败（6次失败→0次）  
**验收**：在Windows CRLF文件上多行search一次成功

---

### E2：start_line / end_line 限定搜索范围（P1）

**问题**：通用代码模式（`.unwrap_or_else`、`pub fn`等）在大文件中多处出现，单行search必然误伤。模型无法告诉工具"只在3845-3855行范围内搜索"。

**现有schema**（file.rs:999-1017）：只有`path`/`search`/`replace`三个字段。

**新增可选参数**：

```rust
// input_schema新增：
"start_line": {
    "type": "integer",
    "description": "将搜索范围限定在此行开始（1-based，含）。与end_line配合使用可精确定位。"
},
"end_line": {
    "type": "integer",
    "description": "将搜索范围限定在此行结束（1-based，含）。"
}
```

**execute逻辑**：

```rust
let start_line = optional_u64(&input, "start_line", 0) as usize;
let end_line = optional_u64(&input, "end_line", 0) as usize;

// 如果指定了行范围，只在该范围内搜索
let (search_target, offset_bytes) = if start_line > 0 {
    // 按行切分，只在指定范围内操作
    let lines: Vec<&str> = contents.lines().collect();
    let s = start_line.saturating_sub(1);
    let e = if end_line > 0 { end_line.min(lines.len()) } else { lines.len() };
    let slice = lines[s..e].join(file_line_ending);
    // 返回切片文本 + 该切片在原文件中的字节偏移
    let byte_offset = lines[..s].iter()
        .map(|l| l.len() + file_line_ending.len())
        .sum::<usize>();
    (slice, byte_offset)
} else {
    (contents.clone(), 0)
};

// 在search_target范围内匹配，替换后拼回contents
```

**改动量**：file.rs `EditFileTool` schema +6行，execute +~20行  
**效果**：V5-1的误伤场景完全可避免——`start_line:3845, end_line:3855`只命中正确位置  
**验收**：同一search字符串在文件中有3处，指定行范围后只替换目标处

---

### E3：错误信息升级——告诉模型为什么失败（P0）

**问题**：现有失败信息只说`"Search string not found in {path}"`，模型不知道是换行符问题还是字符串本身不存在，于是进入盲猜循环，甚至尝试Python/PowerShell旁路。

**现有代码**（file.rs:1057-1061）：
```rust
if count == 0 {
    return Err(ToolError::execution_failed(format!(
        "Search string not found in {}",
        file_path.display()
    )));
}
```

**升级为诊断性错误信息**：

```rust
if count == 0 {
    // 诊断：检查是否是换行符问题
    let has_crlf = contents.contains("\r\n");
    let search_has_lf_only = search.contains('\n') && !search.contains("\r\n");
    
    let hint = if has_crlf && search_has_lf_only {
        " [HINT: 文件使用CRLF(\\r\\n)换行，search字符串使用LF(\\n)——工具已自动转换，若仍失败请检查search字符串内容]"
    } else if search.lines().count() > 1 {
        " [HINT: 多行search未找到——请确认search字符串与文件中的实际内容完全一致，包括缩进和空格]"
    } else {
        " [HINT: 单行search未找到——请用grep_files确认该字符串在文件中的确切内容和位置]"
    };
    
    return Err(ToolError::execution_failed(format!(
        "[NOT_FOUND] search字符串在{}中不存在。{}",
        file_path.display(),
        hint
    )));
}
```

**改动量**：file.rs +~12行  
**效果**：模型收到诊断信息后直接知道下一步怎么做，不会再去尝试Python/PowerShell  
**验收**：CRLF文件搜索失败时，错误信息包含`[HINT: 文件使用CRLF]`

---

### E4：count > 1时警告并支持replace_all / replace_first选择（P1）

**问题**：现有代码`count > 1`时静默替换所有命中（`String::replace`），没有任何警告。模型不知道发生了多处替换，验证时才发现误伤，需要额外的回修工具调用。

**现有代码**（file.rs:1064，1072）：
```rust
let updated = contents.replace(search, replace);
// ...
let summary = format!("Replaced {count} occurrence(s) in {display}");
```

**改进方案**：新增可选参数`replace_mode`，默认行为改为count > 1时返回警告而不是静默替换：

```rust
// schema新增可选字段：
"replace_mode": {
    "type": "string",
    "enum": ["first", "all"],
    "description": "当search有多处命中时：first=只替换第一处（推荐），all=替换所有（需明确指定）。默认：count=1时自动替换；count>1时返回警告要求明确指定。"
}
```

```rust
let replace_mode = optional_str(&input, "replace_mode");

if count > 1 {
    match replace_mode {
        Some("all") => { /* 继续替换所有 */ }
        Some("first") => {
            // 只替换第一处
            let updated = contents.replacen(&search_normalized, &replace_normalized, 1);
            // ...
        }
        None | Some(_) => {
            // 默认：返回警告，要求模型明确指定
            return Err(ToolError::execution_failed(format!(
                "[AMBIGUOUS] search字符串在{}中找到{}处命中。\n\
                请添加 replace_mode 参数：\n\
                - replace_mode: \"first\" — 只替换第一处（行{}）\n\
                - replace_mode: \"all\" — 替换全部{}处\n\
                命中位置（前3处）：{}",
                file_path.display(),
                count,
                // 列出前3处行号
                find_match_line_numbers(&contents, &search_normalized, 3).join(", "),
                count,
                find_match_line_numbers(&contents, &search_normalized, 3)
                    .iter().map(|n| format!("第{}行", n)).collect::<Vec<_>>().join("、")
            )));
        }
    }
}
```

新增辅助函数`find_match_line_numbers()`——返回search命中的行号列表，用于错误信息和诊断。

**改动量**：file.rs +~25行  
**效果**：完全消除误伤场景。模型收到`[AMBIGUOUS]`后，知道有几处命中、在哪些行，可以用`start_line/end_line`精确定位后重试  
**验收**：search有3处命中且未指定replace_mode → 返回`[AMBIGUOUS]`含行号列表；指定`replace_mode:"first"` → 只替换第一处

---

### E5：替换成功后附带命中行号（P2）

**问题**：替换成功后，模型需要再调一次`read_file`确认改对了位置。如果返回中直接包含"在第N行替换了"，这次验证`read_file`可以省掉。

**现有返回**（file.rs:1072）：
```rust
let summary = format!("Replaced {count} occurrence(s) in {display}");
```

**升级为**：
```rust
let match_lines = find_match_line_numbers(&contents, &search_normalized, 5);
let lines_str = match_lines.iter()
    .map(|n| format!("第{}行", n))
    .collect::<Vec<_>>()
    .join("、");
let summary = format!(
    "Replaced {count} occurrence(s) in {display} (位置: {lines_str})"
);
```

**改动量**：file.rs +~5行（复用E4的`find_match_line_numbers`辅助函数）  
**效果**：模型知道替换发生在哪一行，可以直接用`read_file(path, start_line=N-2, limit=10)`精准验证，不需要重读整个文件  
**验收**：替换成功后，返回信息包含`位置: 第3849行`

---

## 辅助函数（共用）

E4和E5共用一个辅助函数，不重复造轮子：

```rust
/// 返回search字符串在contents中命中的行号列表（1-based，最多返回max_results个）
fn find_match_line_numbers(contents: &str, search: &str, max_results: usize) -> Vec<usize> {
    let mut result = Vec::new();
    let mut byte_pos = 0;
    let mut line_num = 1;
    let content_bytes = contents.as_bytes();
    let search_bytes = search.as_bytes();
    
    while byte_pos <= content_bytes.len().saturating_sub(search_bytes.len()) {
        if result.len() >= max_results { break; }
        if content_bytes[byte_pos..].starts_with(search_bytes) {
            result.push(line_num);
            byte_pos += search_bytes.len();
        } else {
            if content_bytes[byte_pos] == b'\n' { line_num += 1; }
            byte_pos += 1;
        }
    }
    result
}
```

---

## 实施顺序

| 批次 | 项 | 改动量 | 收益 |
|:----:|-----|--------|------|
| **J** | E1（换行符自适应）+ E3（诊断错误信息） | ~27行 | 消除V5-1类型的全部失败；模型不再乱试旁路 |
| **K** | E4（count>1警告）+ E2（行范围限定）+ 辅助函数 | ~51行 | 彻底消除误伤；行范围精确定位 |
| **L** | E5（返回命中行号） | ~5行（复用辅助函数） | 省掉验证read_file调用 |

**总改动量**：~83行，全部在`file.rs`的`EditFileTool`实现内，不影响其他工具。

---

## 改动前后对比

| 场景 | 改进前 | 改进后 |
|------|--------|--------|
| Windows CRLF文件多行edit | 必然失败，6次重试 | 自动转换，一次成功 |
| 通用模式误伤 | 静默替换所有，需回修 | [AMBIGUOUS]警告+行号，重试前已知位置 |
| 失败原因不明 | "not found"，模型盲猜 | 诊断性提示，下一步清晰 |
| 验证替换位置 | 需额外read_file | 返回中直接含行号 |
| 精确定位唯一 | 只能靠search字符串唯一性 | start_line/end_line兜底 |

---

## 注意事项

**E1的边界情况**：混合换行符文件（极少见）——以`\r\n`优先判断，有`\r\n`就按CRLF处理。这覆盖了你的项目实际情况（Windows开发环境）。

**E4的默认行为变化**：count>1时由"静默替换所有"变为"返回警告"——这是破坏性变化，但方向是对的。如果有现有的自动化测试依赖多处替换行为，需要在那些调用处明确加`replace_mode:"all"`。

**E5的行号精度**：`find_match_line_numbers`按字节扫描，对于多行search字符串，返回的是匹配起始位置所在行，与编辑器行号一致。
