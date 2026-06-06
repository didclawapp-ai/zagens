# 发布说明

**技能：** `office-release-notes` · **输出：** DOCX

## 作用

将 CHANGELOG、PR 列表或要点整理为面向客户或开发者的**发布说明** DOCX。

## 开始前

- 任务类型：**办公**
- 可选：`CHANGELOG.md`、发布单导出或 `inbox/` 笔记
- 工作区指向代码仓库时，Agent 可 `read_file` 项目 changelog

## 如何运行

1. 点击**发布说明**或输入：
   > 为 v0.7.0 写对外客户版发布说明。
2. 确认：产品名、版本号、发布日期、受众（内部/客户/开发者）。
3. DOCX 输出至 `deliverables/`。

## 典型章节

**版本摘要** · 新功能 · 改进 · 修复 · 已知问题 · 升级指引

## 验收

- 版本号与日期正确
- 功能列表与源 changelog 一致，无臆造条目

## 建议

- 将工作区设为产品仓库根目录以便读取 `CHANGELOG.md`。
- 可在同一 DOCX 中加简短「邮件通报」段落。

相关：[项目汇报 PPT](/zh-Hans/docs/office/skills/project-report) · [文件工具](/zh-Hans/docs/tools/files)
