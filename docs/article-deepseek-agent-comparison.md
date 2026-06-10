# DeepSeek V4 的三种打开方式：花了三天，我测完了市面上三款专配 Agent

> 省流：如果你只需要一个好用的 AI 编程终端，选 CodeWhale。如果你在意 token 成本和性能，选 Reasonix。如果你需要的不只是写代码——还要改文档、出周报、做 PPT，而且希望有沙箱兜底、任务到底有没有做完有机器帮你验证——目前只有 Zagens 能干。

---

## 快速概览

| | Zagens | CodeWhale | Reasonix |
|---|---|---|---|
| **形态** | 桌面应用（Tauri 2）+ CLI | 终端 TUI + VS Code 插件 | 终端 CLI + Electron 桌面 |
| **版本** | v0.7.4（2026-06-10） | v0.8.57（2026-06-10） | v1.5.0（2026-06-10） |
| **GitHub Stars** | 新项目 | 37.8k | 20.8k |
| **语言** | Rust 78% + TypeScript 19% | Rust 94% | Go 71%（v1.0 重写） |
| **安装** | Windows 安装包 / CLI 二进制 | `npm i -g codewhale` | `npm i -g reasonix` |
| **模型** | 自备 Key，DeepSeek + OpenAI 兼容 | 自备 Key，12+ 提供商 | 自备 Key，DeepSeek 为主 |
| **许可** | MIT | MIT | MIT |

---

## 架构差异：终端里跑 vs 桌面上管

这可能是最被忽略但影响最大的区别：

| | Zagens | CodeWhale | Reasonix |
|---|---|---|---|
| **运行时** | 独立 sidecar 进程，loopback 通信 | 终端内直接运行 | 终端内直接运行 |
| **会话模型** | SQLite 持久化、分叉/恢复、按轮回放 | 终端内一次性会话 | 终端内一次性会话（有 checkpoints） |
| **多代理** | CRAFT（4 角色 + 黑板 + fix-loop） | 子代理 | 双模型（executor + planner） |

Zagens 选了最重的架构——一个独立运行的 sidecar 进程，桌面壳通过本地 HTTP 跟它通信。代价是启动开销更大，换来的是：会话不丢、可以回放任何一轮对话、可以从任意历史节点分叉重试。

CodeWhale 和 Reasonix 的选择更轻——就是终端里一个进程。好处是零摩擦启动，坏处是终端关了上下文就没了。

**没有哪种更好，看你用在哪**。日常 prompt→改代码→commit 的短流程，终端更顺手。但如果你跑一个跨多轮、跨多天、中间还想回头看看"模型当时为什么改了那个文件"的任务，sidecar + SQLite 的持久化就是刚需。

---

## 功能覆盖面

| 能力 | Zagens | CodeWhale | Reasonix |
|---|---|---|---|
| 读/写/搜索代码 | ✅ | ✅ | ✅ |
| Shell 执行 | ✅（审批门禁） | ✅ | ✅ |
| Git 操作 | ✅ | ✅ | ✅ |
| MCP / Skills | ✅ | ✅ | ✅ |
| **Office 文档产出** | ✅ `write_office` | ❌ | ❌ |
| **长程任务完成门禁** | ✅ LHT 三层门禁 | ❌ | ❌ |
| **OS 级沙箱** | ✅ Windows + macOS | ❌ | ❌ |
| **会话回放** | ✅ 按轮完整回放 | ❌ | ✅（rewind / checkpoints） |
| **桌面通知 / 托盘** | ✅ | ❌ | ✅（桌面版） |
| **嵌入式 PTY 终端** | ✅（Code 模式内嵌） | 终端即 UI | 终端即 UI |
| 联网搜索 | ✅ 可选 | ✅ | ❌ |

---

## Zagens 真正拉开差距的三个点

### 1. 不止写代码，还能写文档

CodeWhale 和 Reasonix 定位是纯代码 Agent——文件编辑、Shell、Git、搜索，齐了。但现实中的开发工作流不止这些：改完代码要更新接口文档、出周报要填表、给老板汇报要做 PPT。

Zagens 的 `write_office` 是当前三家里**唯一**能直接生成 xlsx/docx/pptx/pdf 的工具。而且 xlsx 走的是纯 Rust 路径（`rust_xlsxwriter`），不需要装 Python 就能跑。DOCX/PPTX/PDF 走捆绑 Python，每次生成后自动存 `.payload.json` 缓存——下次要改这份文档，不用重头生成，直接基于缓存增量编辑。

这不是"多了一个功能"，这是面向完全不同的使用场景。

### 2. 沙箱隔离——不是"建议"，是强制

三家里只有 Zagens 做了 OS 级沙箱：

- **Windows elevated 模式**：受限 Token + ACL + WFP 防火墙规则，能做到工作区外不可写、敏感目录（`.ssh` 等）不可读、出站网络默认阻断（loopback 放行）
- **macOS Seatbelt**：`sandbox-exec` 可用时强制约束
- **安全模式**：read-only / workspace-write / danger-full-access / external-sandbox 四档可切换

CodeWhale 和 Reasonix 在你终端里跑的时候，跟你的用户权限完全一样。它们不是不安全——它们只是没有额外加一层限制。

### 3. 长程任务——最根本的差异

这是三家设计哲学分岔的地方。

CodeWhale 的"宪章"强调的是 agent 的身份和权威边界——**谁在操作、谁说了算**。Reasonix 的"Cache-First"强调的是前缀缓存命中率——**怎么让每轮 token 成本最低**。

Zagens 往上走了一层，它问了一个更根本的问题：**"模型说做完了，真的做完了吗？"**

LHT（Long-Horizon Task）Completion Gates 的设计铁律是：**不允许 LLM 当终审法官**。三层门禁：

- **Layer 1**：模型自检（plan + checklist，CodeWhale/Reasonix 的做法）
- **Layer 2**：硬验收门禁——harness **主动执行**验证命令（跑测试、跑编译），看 exit code
- **Layer 3**：交付物清单对账——纯 Rust 模块做路径/glob 存在性检查，**不经过 LLM**

内部实测 MicroStack02 用例：模型声称 61 文件、7045 行、100% 完成，门禁跑完发现实际覆盖率只有 16.3%。

这就是"模型说做完了"和"机器验证做完了"之间的差距。

---

## Zagens 的短板（如实说）

**不轻量。** CodeWhale 一行 `npm i -g` 就开干，Zagens 要下载安装包或编译——对只是想试一下的人，门槛更高。

**macOS/Linux 还没桌面版。** 目前 Windows 有安装包，其他平台得用 CLI 或源码 build。桌面安装包在规划中。

**社区几乎还没建起来。** 38k stars vs 20k stars vs 新生——遇到问题，CodeWhale 和 Reasonix 有 Discord/Issue/社区教程，Zagens 目前基本靠翻源码和 GitHub Issues。

**不托管模型。** 三家都需要自备 API Key。没有"开箱即用免费额度"。

---

## 该选哪个？

| 你的场景 | 推荐 |
|---|---|
| 日常编码，想要成熟生态和 VS Code 集成 | **CodeWhale** |
| 在意 token 成本，希望长会话省钱，喜欢配置驱动 | **Reasonix** |
| 任务横跨代码和文档产出，需要沙箱兜底、需要机器验证任务完成 | **Zagens**（唯一选择） |

三款工具不是互相替代的关系——它们在对"agent 应该怎么工作"这件事上走了完全不同的路。搞清楚你需要什么，比看谁的 Star 多更重要。
