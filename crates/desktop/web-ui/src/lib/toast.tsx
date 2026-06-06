import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../i18n';

export type ToastVariant = 'success' | 'error' | 'warning' | 'info';

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastOptions {
  variant?: ToastVariant;
  title?: string;
  message: string;
  /** Milliseconds until auto-dismiss; `0` stays until closed. */
  duration?: number;
  action?: ToastAction;
  /** Replaces any existing toast with the same tag. */
  tag?: string;
}

interface ToastRecord extends ToastOptions {
  id: string;
  variant: ToastVariant;
  createdAt: number;
}

const DEFAULT_DURATION: Record<ToastVariant, number> = {
  success: 4000,
  error: 8000,
  warning: 6000,
  info: 4000,
};

const MAX_TOASTS = 4;

/** Stale transport / auth / runtime reachability — auto-dismiss when probe is `connected`. */
export const RUNTIME_TRANSIENT_TAG = 'runtime-transient';

function nextId(): string {
  return `toast-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

interface ToastApi {
  push: (options: ToastOptions) => string;
  dismiss: (id: string) => void;
  dismissByTag: (tag: string) => void;
  dismissAll: () => void;
}

const ToastContext = createContext<ToastApi | null>(null);

let globalApi: ToastApi | null = null;

function bindGlobalApi(api: ToastApi | null) {
  globalApi = api;
}

export function toast(message: string): string;
export function toast(options: ToastOptions): string;
export function toast(input: string | ToastOptions): string {
  const options: ToastOptions =
    typeof input === 'string' ? { message: input, variant: 'error' } : input;
  if (!globalApi) {
    console.warn('[toast]', options.message);
    return '';
  }
  return globalApi.push(options);
}

toast.success = (message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) =>
  toast({ variant: 'success', message, ...opts });

toast.error = (message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) =>
  toast({ variant: 'error', message, ...opts });

toast.warning = (message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) =>
  toast({ variant: 'warning', message, ...opts });

toast.info = (message: string, opts?: Omit<ToastOptions, 'message' | 'variant'>) =>
  toast({ variant: 'info', message, ...opts });

toast.dismiss = (id: string) => {
  globalApi?.dismiss(id);
};

toast.dismissByTag = (tag: string) => {
  globalApi?.dismissByTag(tag);
};

toast.dismissAll = () => {
  globalApi?.dismissAll();
};

const VARIANT_STYLES: Record<
  ToastVariant,
  { box: string; icon: string; iconLabel: string }
> = {
  success: {
    box: 'border-success/35 bg-success-bg text-t-text',
    icon: 'text-success',
    iconLabel: '✓',
  },
  error: {
    box: 'border-t-error/35 bg-error-bg text-t-text',
    icon: 'text-t-error',
    iconLabel: '✕',
  },
  warning: {
    box: 'border-amber/35 bg-amber-bg text-t-text',
    icon: 'text-amber-text',
    iconLabel: '!',
  },
  info: {
    box: 'border-card-border bg-card text-t-text-secondary',
    icon: 'text-accent',
    iconLabel: 'i',
  },
};

function ToastViewport({
  items,
  onDismiss,
}: {
  items: ToastRecord[];
  onDismiss: (id: string) => void;
}) {
  const { t } = useT();
  if (items.length === 0) {
    return null;
  }

  return (
    <div
      className="pointer-events-none fixed bottom-4 right-4 z-[10000] flex w-[min(24rem,calc(100vw-2rem))] flex-col items-stretch gap-2"
      aria-live="polite"
      aria-relevant="additions"
    >
      {items.map((item) => {
        const styles = VARIANT_STYLES[item.variant];
        return (
          <div
            key={item.id}
            role="status"
            className={`pointer-events-auto flex w-full items-start gap-2.5 rounded-xl border px-3.5 py-2.5 text-sm shadow-lg ${styles.box}`}
            style={{ boxShadow: 'var(--color-shadow-lg)' }}
          >
            <span
              className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[11px] font-bold ${styles.icon}`}
              aria-hidden
            >
              {styles.iconLabel}
            </span>
            <div className="min-w-0 flex-1">
              {item.title ? (
                <p className="font-medium text-t-text leading-snug">{item.title}</p>
              ) : null}
              <p className={`leading-relaxed ${item.title ? 'mt-0.5 text-t-text-secondary' : ''}`}>
                {item.message}
              </p>
              {item.action ? (
                <button
                  type="button"
                  className="mt-2 text-xs font-medium text-accent hover:underline"
                  onClick={() => {
                    item.action?.onClick();
                  }}
                >
                  {item.action.label}
                </button>
              ) : null}
            </div>
            <button
              type="button"
              className="shrink-0 rounded-md p-1 text-t-text-muted hover:bg-hover hover:text-t-text transition-colors"
              aria-label={t('common.close')}
              onClick={() => onDismiss(item.id)}
            >
              <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M4 4l8 8M12 4l-8 8" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        );
      })}
    </div>
  );
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastRecord[]>([]);
  const timersRef = useRef<Map<string, number>>(new Map());

  const clearTimer = useCallback((id: string) => {
    const handle = timersRef.current.get(id);
    if (handle != null) {
      window.clearTimeout(handle);
      timersRef.current.delete(id);
    }
  }, []);

  const dismiss = useCallback(
    (id: string) => {
      clearTimer(id);
      setItems((prev) => prev.filter((x) => x.id !== id));
    },
    [clearTimer],
  );

  const dismissByTag = useCallback(
    (tag: string) => {
      setItems((prev) => {
        for (const x of prev) {
          if (x.tag === tag) {
            clearTimer(x.id);
          }
        }
        return prev.filter((x) => x.tag !== tag);
      });
    },
    [clearTimer],
  );

  const dismissAll = useCallback(() => {
    for (const id of [...timersRef.current.keys()]) {
      clearTimer(id);
    }
    setItems([]);
  }, [clearTimer]);

  const scheduleDismiss = useCallback(
    (id: string, duration: number) => {
      if (duration <= 0) {
        return;
      }
      clearTimer(id);
      const handle = window.setTimeout(() => dismiss(id), duration);
      timersRef.current.set(id, handle);
    },
    [clearTimer, dismiss],
  );

  const push = useCallback(
    (options: ToastOptions): string => {
      const variant = options.variant ?? 'error';
      const duration =
        options.duration !== undefined
          ? options.duration
          : options.action
            ? 0
            : DEFAULT_DURATION[variant];
      const id = nextId();
      const record: ToastRecord = {
        ...options,
        variant,
        id,
        createdAt: Date.now(),
      };

      setItems((prev) => {
        let next = prev;
        if (options.tag) {
          for (const x of prev) {
            if (x.tag === options.tag) {
              clearTimer(x.id);
            }
          }
          next = prev.filter((x) => x.tag !== options.tag);
        }
        next = [...next, record];
        while (next.length > MAX_TOASTS) {
          const removed = next.shift();
          if (removed) {
            clearTimer(removed.id);
          }
        }
        return next;
      });

      scheduleDismiss(id, duration);
      return id;
    },
    [clearTimer, scheduleDismiss],
  );

  const api = useMemo<ToastApi>(
    () => ({ push, dismiss, dismissByTag, dismissAll }),
    [push, dismiss, dismissByTag, dismissAll],
  );

  useEffect(() => {
    bindGlobalApi(api);
    return () => bindGlobalApi(null);
  }, [api]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      {typeof document !== 'undefined' &&
        createPortal(<ToastViewport items={items} onDismiss={dismiss} />, document.body)}
    </ToastContext.Provider>
  );
}

export function useToastApi(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToastApi must be used within ToastProvider');
  }
  return ctx;
}
