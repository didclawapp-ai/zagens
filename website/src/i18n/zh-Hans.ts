import type { SiteCopy } from './index';
import { en } from './en';

/** Simplified Chinese — preview launch copy. */
export const zhHans: SiteCopy = {
  ...en,
  meta: {
    siteName: 'Zagens',
    defaultDescription:
      '面向 DeepSeek V4 生态的 Agent Harness 桌面端 — Windows 预览版。',
  },
  nav: {
    home: '首页',
    download: '下载',
    install: '安装指引',
    privacy: '隐私政策',
    terms: '使用条款',
  },
  common: {
    previewBadge: '预览版',
    windowsOnly: '仅 Windows x64（预览）',
    learnMore: '了解更多',
    backHome: '返回首页',
    lastUpdated: '最后更新',
  },
  home: {
    title: '桌面 Agent Harness',
    subtitle:
      '面向 DeepSeek V4 生态的 Agent Harness 桌面端，专注长程编程、办公工作流与工作区感知的 Agent 任务。',
    ctaDownload: '下载 Windows 版',
    ctaInstall: '安装指引',
    heroAlt: 'Zagens 桌面应用预览',
    heroHighlights: ['本地优先 · 无遥测', 'Windows 10 / 11（x64）', 'DeepSeek V4 · 兼容 OpenAI 接口'],
    disclaimer: '与 DeepSeek Inc. 无隶属关系。使用前需自备 DeepSeek（或兼容）API Key。',
    featuresEyebrow: '为真实工作打造',
    featuresTitle: '一个窗口搞定全部',
    featuresSubtitle: '聊天、编码、文档共享同一工作区 —— 无需在多个工具间来回切换。',
    features: [
      {
        title: '本地 Sidecar',
        body: '聊天、工具与会话通过本机 HTTP sidecar 运行 — 非托管沙箱。',
      },
      {
        title: '代码 & 办公模式',
        body: '按任务类型切换工具面：工程工作区（终端、diff）或文档向办公流程。',
      },
      {
        title: '长程任务',
        body: '清单、完成门禁与会话回放，帮助多步重构保持上下文不丢失。',
        badge: '实验性',
      },
      {
        title: '工作区原生',
        body: '文件树、预览、diff 面板、回放、MCP、技能与托盘通知，同一窗口完成。',
      },
    ],
    howEyebrow: '快速上手',
    howTitle: '三步即可运行',
    howSteps: [
      {
        title: '下载并解锁',
        body: '下载 zip，右键「解除锁定」后解压并运行安装器。缺少 WebView2 时会自动安装。',
      },
      {
        title: '填入 API Key',
        body: '首次启动时粘贴 DeepSeek 或 OpenAI 兼容 Key。Key 仅保存在本机，Zagens 不会读取。',
      },
      {
        title: '开始任务',
        body: '选择代码或办公模式，指向工作区，让 Agent 规划、编辑、执行，并支持完整回放。',
      },
    ],
    capabilitiesEyebrow: '开箱即用',
    capabilitiesTitle: '完整的 Agent 工作区',
    capabilitiesSubtitle: '桌面 Harness 该有的能力一应俱全 —— 无需依赖任何云端账号。',
    capabilitiesScreenshotAlt: 'Zagens 正在执行长程任务，Checklist、Audit、Sub-agents 面板同时展开',
    capabilities: [
      '文件树 & 实时预览',
      '并排 diff 面板',
      '会话回放 & 审计日志',
      '内置终端',
      'MCP 服务',
      '技能（Skills）',
      '系统托盘通知',
      '代码 & 办公双模式',
    ],
    requirementsTitle: '下载前请确认',
    requirements: [
      'Windows 10 或 11（64 位）',
      'DeepSeek 或 OpenAI 兼容 API Key（仅存本机）',
      '模型调用需联网；缺少 WebView2 时安装器会自动安装',
    ],
    ctaTitle: '准备好让 Agent 开始工作了吗？',
    ctaSubtitle: '下载 Windows 预览版，几分钟内跑通你的第一个长程任务。',
    faqTitle: '常见问题',
    faq: [
      {
        q: '长程任务（LHT）功能完善吗？',
        a: 'LHT 是 Zagens 最核心也最复杂的部分，目前仍在持续打磨中。任务规划、Checklist 跟踪、会话回放与审计日志等核心流程已可用，但在复杂的多步场景下可能会遇到一些粗糙的边角情况。我们会在每个预览版本中持续改进，欢迎反馈问题和建议。',
      },
      {
        q: 'Windows SmartScreen 提示安全警告，安全吗？',
        a: 'Zagens 预览版尚未代码签名，这是早期预览的正常现象 —— 并非病毒告警。建议下载 zip 包，先「解除锁定」再解压。每个文件都附带 SHA-256 校验值，可自行核对完整性。',
      },
      {
        q: '需要 DeepSeek 账号或 API Key 吗？',
        a: '需要。请自备 DeepSeek 或 OpenAI 兼容的 API Key。它仅保存在本机（可用时使用系统密钥链），不会上传到任何 Zagens 服务器。',
      },
      {
        q: '我的数据会被上传吗？',
        a: 'Zagens 本地优先。工作区文件在本机读写，只有当你调用的工具需要联网时（如配置的模型服务商或 web 搜索）数据才会出网。',
      },
      {
        q: '支持哪些 AI 模型？',
        a: 'Zagens 以 DeepSeek V4（deepseek-chat / deepseek-reasoner）为主力模型。同时兼容所有 OpenAI 接口格式的端点 —— 只需填入服务商的 Base URL 即可切换。后续版本计划支持更多模型系列。',
      },
      {
        q: '支持 macOS 或 Linux 吗？',
        a: '暂不支持。预览版仅面向 Windows 10/11（x64）。其他平台可能在预览阶段之后跟进。',
      },
    ],
  },
  download: {
    title: '下载 Zagens',
    subtitle: 'Windows 预览构建。推荐使用 zip 包，可减少 SmartScreen 干扰。',
    recommended: '推荐',
    zipTitle: '安装包 zip',
    zipHint: '解压前请先「解除锁定」— 见安装指引。',
    exeTitle: '直接下载安装器',
    exeHint: '首次运行可能出现 SmartScreen 提示。',
    sha256: 'SHA-256',
    verifyHint: 'PowerShell 校验：Get-FileHash .\\file -Algorithm SHA256',
    releaseNotes: 'Release 标签',
    noReleaseYet:
      '首个 GitHub Release 发布前，下载链接为占位。打 tag 后请运行 npm run sync:manifest 同步。',
    apiKeyNote: '安装完成后，在 Zagens 引导流程中填写 API Key。',
  },
  install: {
    title: 'Windows 安装指引',
    subtitle: 'Zagens 尚未代码签名，预览版正常现象 — 不是病毒告警。',
    recommendedTitle: '推荐：下载 zip → 解除锁定 → 解压 → 安装',
    steps: [
      '从下载页获取 Zagens_*_x64-setup.exe.zip。',
      '右键 zip → 属性 → 勾选底部「解除锁定」→ 确定。',
      '解压 zip（右键 → 全部解压）。务必先解锁再解压。',
      '双击解压后的 Zagens_*_x64-setup.exe 完成安装。',
    ],
    unblockWarning:
      '重要：必须先解锁 zip 再解压，否则 Windows 会把「网络来源标记」传染给安装器。',
    alternativeTitle: '备选：直接运行 .exe',
    alternativeSteps: [
      '双击 setup.exe。',
      '出现 SmartScreen 蓝框时，点击「更多信息」。',
      '点击「仍要运行」。',
    ],
    verifyTitle: '校验完整性（可选）',
    verifyBody: '每个文件附带 .sha256 校验值，哈希一致即表示下载完整未被篡改。',
    requirementsTitle: '系统要求',
    requirements: [
      'Windows 10 / 11（x64）',
      'WebView2 — 缺失时安装器自动安装（需联网）',
      '内置 runtime sidecar — 无需单独安装 Python',
    ],
    whyUnsignedTitle: '为何未签名？',
    whyUnsignedBody:
      '代码签名证书（尤其 EV）成本较高。预览版采用「zip 解锁 + 校验值」零成本方案；后续可能接入 Microsoft Store 或 Azure Trusted Signing。',
  },
  privacy: {
    title: '隐私政策',
    intro:
      '本文档说明 Zagens（预览版）在本机如何处理数据。仅为发布前草案，不构成法律意见 — 上线前请法务审阅。',
    sections: [
      {
        heading: '本地优先',
        body: 'Zagens 在本机运行 runtime sidecar。工作区文件在本地读写；仅当你调用的工具访问网络时（如 web 搜索或模型 API）才会出网。',
      },
      {
        heading: 'API Key',
        body: '你输入的 Provider API Key 保存在本机（可用时使用系统密钥链）。预览版不提供 Zagens 云账号体系。',
      },
      {
        heading: '模型服务商',
        body: '发送对话时，请求会发往你配置的 AI 服务商（如 DeepSeek）。其服务器上的数据处理适用该服务商隐私政策。',
      },
      {
        heading: '遥测',
        body: '预览版不包含中心化 Zagens 分析管道。应用内用量统计来自本机会话与审计日志。',
      },
      {
        heading: '联系',
        body: '问题咨询：上线前请将此处替换为正式支持邮箱。',
      },
    ],
  },
  terms: {
    title: '使用条款',
    intro: '预览软件 — 按「现状」提供。下载或使用 Zagens 预览版即表示同意以下摘要。',
    sections: [
      {
        heading: '预览状态',
        body: 'Zagens v0.x 预览版的行为、API 与配置可能随时变更，不提供生产级 SLA。',
      },
      {
        heading: '许可',
        body: 'Zagens 桌面为专有软件，见仓库 LICENSE。嵌入式 runtime 含 MIT 第三方代码，见 NOTICE.md。',
      },
      {
        heading: '你的责任',
        body: '你须自行承担 API 费用、本机安全、工具操作（含 shell 命令）审查，以及遵守服务商条款。',
      },
      {
        heading: '无隶属关系',
        body: 'Zagens 与 DeepSeek Inc. 及文档中提及的其他模型服务商无隶属关系。',
      },
      {
        heading: '责任限制',
        body: '在法律允许范围内，作者不对预览软件使用导致的数据丢失、停机或损害承担责任。',
      },
    ],
  },
  footer: {
    tagline: 'Desktop agent harness · 预览版',
    copyright: 'Zagens Contributors',
  },
};
