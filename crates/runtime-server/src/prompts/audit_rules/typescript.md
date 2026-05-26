# TypeScript 代码事实核查规则

审查报告中引用 TypeScript 源代码时，以下条件必须满足：

## 1. 文件路径
- 必须以 `.ts` 或 `.tsx` 结尾
- 必须为仓库相对路径

## 2. 行号
- 必须为具体数字或数字范围

## 3. 内容匹配（机械检查 — 字符串包含）
- `read_file` 读取对应行
- 该行内容必须在以下类别之一内：
  - `function` / `const` / `let` 定义
  - `interface` / `type` / `enum` 定义
  - React hook 调用：`useState` / `useEffect` / `useCallback` / `useMemo`
  - `invoke('xxx')` 调用（Tauri bridge）
  - `import` 语句
  - 类型注解（`: Type` / `as Type` / `satisfies Type`）

## 4. strict 模式下的额外要求
- 项目 `tsconfig.json` 声明 `strict: true`
- 发现不得以"使用了 any"作为独立问题而不引用具体位置
- 如果发现声称"缺少类型"，必须引用具体的变量/函数定义行

## 5. 涉及 Tauri invoke
- 见 tauri_bridge.md
