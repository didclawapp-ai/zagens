/** Flat stroke icons — match Sidebar / Composer (24×24, stroke 1.6, no fill). */

import type { ReactNode, SVGProps } from 'react';

const stroke = { fill: 'none' as const, stroke: 'currentColor', strokeWidth: 1.6, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const };

type IconProps = SVGProps<SVGSVGElement> & { className?: string };

function Icon({ className = 'size-4 shrink-0', children, ...rest }: IconProps & { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" className={className} aria-hidden {...stroke} {...rest}>
      {children}
    </svg>
  );
}

export function IconFolder({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
    </Icon>
  );
}

export function IconFolderOpen({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M5 19h14a2 2 0 002-2V9a2 2 0 00-2-2h-5l-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      <path d="M3 10h18" />
    </Icon>
  );
}

export function IconFile({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" />
      <path d="M14 2v6h6" />
    </Icon>
  );
}

export function IconFileCode({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" />
      <path d="M14 2v6h6" />
      <path d="M10 13l-2 2 2 2M14 13l2 2-2 2" />
    </Icon>
  );
}

export function IconFileMarkdown({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" />
      <path d="M14 2v6h6" />
      <path d="M8 13h2l1 3 1-3h2" />
    </Icon>
  );
}

export function IconFileImage({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" />
      <path d="M14 2v6h6" />
      <circle cx="10" cy="13" r="1.5" />
      <path d="M6 18l4-4 3 3 5-6 4 5" />
    </Icon>
  );
}

export function IconFileConfig({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" />
      <path d="M14 2v6h6" />
      <circle cx="12" cy="14" r="2" />
      <path d="M12 12v-1M12 17v-1M14.9 13.5l.7-.7M10.4 15l-.7.7M15.5 15l.7.7M10.4 13.5l-.7-.7" />
    </Icon>
  );
}

export function IconChevronRight({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M9 6l6 6-6 6" />
    </Icon>
  );
}

export function IconChevronUp({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M6 15l6-6 6 6" />
    </Icon>
  );
}

export function IconRefresh({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M4 12a8 8 0 0113.4-5.9M20 7v4h-4" />
      <path d="M20 12a8 8 0 01-13.4 5.9M4 17v-4h4" />
    </Icon>
  );
}

export function IconSearch({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <circle cx="11" cy="11" r="6" />
      <path d="M16 16l4 4" />
    </Icon>
  );
}

export function IconCopy({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <rect x="8" y="8" width="12" height="12" rx="1" />
      <path d="M4 16V6a2 2 0 012-2h10" />
    </Icon>
  );
}

export function IconExternalFolder({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z" />
      <path d="M12 11v6M9 14h6" />
    </Icon>
  );
}

export function IconEye({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7z" />
      <circle cx="12" cy="12" r="2.5" />
    </Icon>
  );
}

export function IconEyeOff({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M17.94 17.94A10.07 10.07 0 0112 19c-7 0-10-7-10-7a18.45 18.45 0 015.06-6.94" />
      <path d="M1 1l22 22M10.71 10.71a3 3 0 004.24 4.24" />
      <path d="M9.88 5.1A10.07 10.07 0 0112 5c7 0 10 7 10 7a18.5 18.5 0 01-2.16 3.19" />
    </Icon>
  );
}

export function IconPlus({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M12 5v14M5 12h14" />
    </Icon>
  );
}

export function IconList({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M9 6h11M9 12h11M9 18h11M5 6h.01M5 12h.01M5 18h.01" />
    </Icon>
  );
}

export function IconTree({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M12 4v3M8 11H5a2 2 0 000 4h3M12 11h7a2 2 0 010 4h-5M12 18v2" />
    </Icon>
  );
}

export function IconHome({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M4 10.5L12 4l8 6.5V20a1 1 0 01-1 1h-5v-6H10v6H5a1 1 0 01-1-1v-9.5z" />
    </Icon>
  );
}

export function IconAlert({ className }: { className?: string }) {
  return (
    <Icon className={className}>
      <path d="M12 9v4M12 17h.01" />
      <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
    </Icon>
  );
}

export type FileIconKind = 'folder' | 'code' | 'markdown' | 'image' | 'config' | 'file';

export function fileIconKindForName(name: string, isDir: boolean): FileIconKind {
  if (isDir) return 'folder';
  const ext = (name.split('.').pop() ?? '').toLowerCase();
  if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'bmp', 'ico'].includes(ext)) return 'image';
  if (['md', 'mdx', 'markdown'].includes(ext)) return 'markdown';
  if (
    ['rs', 'ts', 'tsx', 'js', 'jsx', 'py', 'go', 'java', 'c', 'cpp', 'h', 'cs', 'swift', 'kt', 'rb', 'php', 'vue', 'svelte', 'sql', 'sh', 'bash', 'zsh', 'ps1'].includes(ext)
  ) {
    return 'code';
  }
  if (['toml', 'json', 'yaml', 'yml', 'lock', 'ini', 'cfg', 'conf', 'env', 'example'].includes(ext)) {
    return 'config';
  }
  return 'file';
}

export function WorkspaceEntryIcon({
  name,
  isDir,
  className = 'size-4 shrink-0',
}: {
  name: string;
  isDir: boolean;
  className?: string;
}) {
  const kind = fileIconKindForName(name, isDir);
  const color =
    kind === 'folder'
      ? 'text-accent/90'
      : kind === 'code'
        ? 'text-sky-500/80 dark:text-sky-400/80'
        : kind === 'markdown'
          ? 'text-violet-500/80 dark:text-violet-400/80'
          : kind === 'image'
            ? 'text-emerald-500/80 dark:text-emerald-400/80'
            : kind === 'config'
              ? 'text-amber-600/80 dark:text-amber-400/80'
              : 'text-t-text-muted';

  const cls = `${className} ${color}`;
  switch (kind) {
    case 'folder':
      return <IconFolder className={cls} />;
    case 'code':
      return <IconFileCode className={cls} />;
    case 'markdown':
      return <IconFileMarkdown className={cls} />;
    case 'image':
      return <IconFileImage className={cls} />;
    case 'config':
      return <IconFileConfig className={cls} />;
    default:
      return <IconFile className={cls} />;
  }
}
