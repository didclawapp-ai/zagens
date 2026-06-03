export const en = {
  meta: {
    siteName: 'Zagens',
    defaultDescription:
      'A desktop Agent Harness built for the DeepSeek V4 ecosystem — preview release for Windows.',
  },
  nav: {
    home: 'Home',
    download: 'Download',
    install: 'Install guide',
    privacy: 'Privacy',
    terms: 'Terms',
  },
  common: {
    previewBadge: 'Preview',
    windowsOnly: 'Windows x64 only (preview)',
    learnMore: 'Learn more',
    backHome: 'Back to home',
    lastUpdated: 'Last updated',
    supportEmail: 'didclawapp@gmail.com',
    contact: 'Contact',
  },
  home: {
    title: 'Desktop agent harness',
    subtitle:
      'A desktop Agent Harness built for the DeepSeek V4 ecosystem — long-horizon coding, office workflows, and workspace-aware agent tasks.',
    ctaDownload: 'Download for Windows',
    ctaInstall: 'Install guide',
    heroAlt: 'Zagens desktop app preview',
    heroHighlights: ['Local-first · no telemetry', 'Windows 10 / 11 (x64)', 'DeepSeek V4 · OpenAI-compatible'],
    disclaimer:
      'Not affiliated with DeepSeek Inc. You need your own DeepSeek (or compatible) API key.',
    featuresEyebrow: 'Built for real work',
    featuresTitle: 'One window for the whole job',
    featuresSubtitle:
      'Chat, code, and documents share a single workspace — no context switching between half a dozen tools.',
    features: [
      {
        title: 'Local sidecar',
        body: 'Chat, tools, and sessions run through an embedded HTTP sidecar on your machine — not a hosted sandbox.',
      },
      {
        title: 'Code & Office modes',
        body: 'Switch task types with different tool surfaces: engineering workspaces with terminal and diff, or document-centric office flows.',
      },
      {
        title: 'Long-horizon tasks',
        body: 'Checklists, completion gates, and replay help multi-step refactors stay on track without losing context.',
        badge: 'Experimental',
      },
      {
        title: 'Workspace-native',
        body: 'File tree, previews, diff panel, session replay, MCP, skills, and system-tray notifications in one window.',
      },
    ],
    howEyebrow: 'Get started',
    howTitle: 'Running in three steps',
    howSteps: [
      {
        title: 'Download & unblock',
        body: 'Grab the zip, right-click → Unblock, then extract and run the installer. WebView2 installs automatically if missing.',
      },
      {
        title: 'Add your API key',
        body: 'On first launch, paste a DeepSeek or OpenAI-compatible key. It stays on your device — Zagens never sees it.',
      },
      {
        title: 'Start a task',
        body: 'Pick Code or Office mode, point it at a workspace, and let the agent plan, edit, and run with full replay.',
      },
    ],
    capabilitiesEyebrow: 'In the box',
    capabilitiesTitle: 'A complete agent workspace',
    capabilitiesSubtitle: 'Everything you expect from a desktop harness — nothing hidden behind a cloud account.',
    capabilitiesScreenshotAlt: 'Zagens running a long-horizon task with checklist, audit, and sub-agents panels open',
    capabilities: [
      'File tree & live preview',
      'Side-by-side diff panel',
      'Session replay & audit log',
      'Integrated terminal',
      'MCP servers',
      'Skills',
      'System-tray notifications',
      'Code & Office modes',
    ],
    requirementsTitle: 'Before you download',
    requirements: [
      'Windows 10 or 11 (64-bit)',
      'A DeepSeek or OpenAI-compatible API key (stored locally)',
      'Internet for model calls; WebView2 installs automatically if missing',
    ],
    ctaTitle: 'Ready to put an agent to work?',
    ctaSubtitle: 'Download the Windows preview and run your first long-horizon task in minutes.',
    faqTitle: 'Frequently asked questions',
    faq: [
      {
        q: 'How complete is the Long-Horizon Task (LHT) feature?',
        a: 'LHT is the most ambitious part of Zagens and is still actively being developed. The core loop — task planning, checklist tracking, session replay, and audit log — is functional, but you may hit rough edges in complex multi-step scenarios. We ship improvements with every preview build. Feedback and bug reports are very welcome.',
      },
      {
        q: 'Why does Windows SmartScreen warn me? Is it safe?',
        a: 'Zagens is not code-signed yet, which is normal for an early preview — it is not a virus warning. Download the zip, unblock it, then extract. Every artifact ships with a SHA-256 checksum so you can verify integrity.',
      },
      {
        q: 'Do I need a DeepSeek account or API key?',
        a: 'Yes. You bring your own DeepSeek or OpenAI-compatible API key. It is stored locally on your device (OS keyring where available) and never sent to a Zagens server.',
      },
      {
        q: 'Is my data sent anywhere?',
        a: 'Zagens is local-first. Workspace files are read and written on your machine. Data only leaves your device when a tool you invoke calls the network — for example your configured model provider or a web search.',
      },
      {
        q: 'Which AI models are supported?',
        a: 'Zagens is built around DeepSeek V4 (deepseek-chat / deepseek-reasoner) as the primary model. Any OpenAI-compatible endpoint works too — just point it at your provider\'s base URL. Other model families may be added in future releases.',
      },
      {
        q: 'Is macOS or Linux supported?',
        a: 'Not yet. The preview targets Windows 10/11 (x64). Other platforms may follow after the preview phase.',
      },
    ],
  },
  download: {
    title: 'Download Zagens',
    subtitle: 'Preview builds for Windows. We recommend the zip package to avoid SmartScreen friction.',
    recommended: 'Recommended',
    zipTitle: 'Installer zip',
    zipHint: 'Unblock the zip before extracting — see the install guide.',
    exeTitle: 'Direct installer',
    exeHint: 'May show a SmartScreen prompt on first run.',
    sha256: 'SHA-256',
    verifyHint: 'Compare with PowerShell: Get-FileHash .\\file -Algorithm SHA256',
    releaseNotes: 'Version',
    noReleaseYet:
      'Installer files are not available yet. Upload them to the site download folder and redeploy.',
    apiKeyNote: 'After install, open Zagens and enter your API key in the onboarding flow.',
    count: {
      label: '{n} downloads',
      loading: 'Loading download count…',
      unavailable: '',
    },
  },
  install: {
    title: 'Windows install guide',
    subtitle: 'Zagens is not code-signed yet. This is normal for an early preview — not a virus warning.',
    recommendedTitle: 'Recommended: zip → Unblock → extract → install',
    steps: [
      'Download the Zagens_*_x64-setup.exe.zip file from the download page.',
      'Right-click the zip → Properties → tick Unblock at the bottom → OK.',
      'Extract the zip (Extract All). Do this only after unblocking.',
      'Double-click Zagens_*_x64-setup.exe inside the extracted folder.',
    ],
    unblockWarning:
      'Important: unblock the zip before extracting. Windows otherwise copies the "Mark of the Web" onto the installer.',
    alternativeTitle: 'Alternative: run the .exe directly',
    alternativeSteps: [
      'Double-click the setup.exe.',
      'When SmartScreen appears, click More info.',
      'Click Run anyway.',
    ],
    verifyTitle: 'Verify integrity (optional)',
    verifyBody:
      'Each artifact ships with a .sha256 file. Matching hashes confirm the download is intact.',
    requirementsTitle: 'System requirements',
    requirements: [
      'Windows 10 / 11 (x64)',
      'WebView2 — auto-installed during setup if missing (network required)',
      'Bundled runtime sidecar — no separate Python install',
    ],
    whyUnsignedTitle: 'Why unsigned?',
    whyUnsignedBody:
      'Code-signing certificates are costly. For this preview we ship with unblock + checksums. Microsoft Store or Azure Trusted Signing may follow later.',
  },
  privacy: {
    title: 'Privacy policy',
    intro:
      'This policy describes how Zagens (preview) handles data on your device.',
    sections: [
      {
        heading: 'Local-first by design',
        body: 'Zagens runs a local runtime sidecar on your computer. Your workspace files are read and written locally unless a tool you invoke accesses the network (for example web search or model APIs).',
      },
      {
        heading: 'API keys',
        body: 'Provider API keys you enter are stored on this device (OS keyring where available). Zagens does not operate a cloud account system for preview builds.',
      },
      {
        heading: 'Model providers',
        body: 'When you send a prompt, requests go to the AI provider you configured (for example DeepSeek). Their privacy policy applies to data processed on their servers.',
      },
      {
        heading: 'Telemetry',
        body: 'Preview builds do not include a centralized Zagens analytics pipeline. Usage metrics shown in the app are derived from local session and audit logs on your machine.',
      },
      {
        heading: 'Contact',
        body: 'Questions, feedback, or privacy requests:',
      },
    ],
  },
  terms: {
    title: 'Terms of use',
    intro:
      'Preview software — provided as-is. By downloading or using Zagens preview you agree to the following summary.',
    sections: [
      {
        heading: 'Preview status',
        body: 'Zagens v0.x preview releases may change behavior, APIs, and configuration without notice. No production SLA is offered.',
      },
      {
        heading: 'License',
        body: 'Zagens desktop is proprietary software. See the repository LICENSE. Embedded runtime components include MIT-licensed third-party code documented in NOTICE.md.',
      },
      {
        heading: 'Your responsibilities',
        body: 'You are responsible for API usage charges, securing your machine, reviewing tool actions (including shell commands), and complying with your provider terms.',
      },
      {
        heading: 'No affiliation',
        body: 'Zagens is not affiliated with DeepSeek Inc. or other model providers mentioned in documentation.',
      },
      {
        heading: 'Limitation of liability',
        body: 'To the maximum extent permitted by law, the authors are not liable for data loss, downtime, or damages arising from use of preview software.',
      },
      {
        heading: 'Contact',
        body: 'Support and feedback:',
      },
    ],
  },
  footer: {
    tagline: 'Desktop agent harness · Preview',
    copyright: 'Zagens Contributors',
  },
} as const;
