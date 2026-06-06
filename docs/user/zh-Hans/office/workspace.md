# 办公工作区

办公模式使用**面向文档**的目录结构，而不是 git 工程仓库。

## 推荐结构

```
my-office-workspace/
  inbox/          # 简报、纪要、导出的邮件（DOCX、MD、PDF 等）
  data/           # 价目表、参考表（CSV、XLSX）
  deliverables/   # Agent 输出（首次 write_office 时创建）
```

| 目录 | 用途 |
|------|------|
| `inbox/` | Agent 读取的输入 — 部门日报、会议材料 |
| `data/` | 结构化表格 — 价目表、指标导出 |
| `deliverables/` | Agent 写入的 DOCX / XLSX / PPTX / PDF |

`inbox/` 与 `data/` **不会**自动初始化 — 需手动创建或复制 fixtures（[应用场景](/zh-Hans/use-cases) 页提供 zip）。详见[交付物](/zh-Hans/docs/office/deliverables)。

## 预览与交付

- 点击 `deliverables/` 下文件可预览提取文本
- 需要完整版式时用[交付物](/zh-Hans/docs/office/deliverables)中的系统打开
- `write_office` 完成后会高亮新文件

## 办公模式不做什么

- 不提供嵌入式终端或全仓库重构 — 工程任务请用**代码**模式
- 交付**文件**，而不是在聊天里贴长文

试跑 [P0 示范](/zh-Hans/docs/office/scenarios)，或直接进入：

- [竞品/行业动态](/zh-Hans/docs/office/p0-competitive)
- [经营日报汇总](/zh-Hans/docs/office/p0-executive)
- [生产/品质晨报](/zh-Hans/docs/office/p0-production)
- [客户报价单](/zh-Hans/docs/office/p0-quote)
