# 工作区快照

Zagens 可在 `~/.zagens/snapshots/` 维护**工作区快照**（side-git），与仓库 `.git` 独立。

## 用途

在 Agent 探索性修改后，无需立即 git commit 即可回滚文件状态。

## 恢复

通过 UI 或运行时提供的恢复/回退回合操作回到捕获点。

快照**不能**替代 git — 正式项目请两者并用。

## 范围

- 覆盖当前会话配置的工作区目录
- 不包含 API Key 或全局 `~/.zagens/config.toml`

## 配置

保留策略等见 `~/.zagens/config.toml` 的 `[snapshots]`（仓库有 `config.example.toml`）。

## 建议

- 大改前可先 git commit。
- 办公场景可在重写前备份 `deliverables/`。

相关：[工作区概览](/zh-Hans/docs/workspace/overview) · [Diff](/zh-Hans/docs/workspace/diff)
