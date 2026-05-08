import { useState, useRef, useEffect } from 'react';

interface Props {
  onSend: (text: string) => void;
  onCancel?: () => void;
  disabled: boolean;
  autoApprove: boolean;
  onAutoApproveChange: (value: boolean) => void;
}

export default function Composer({
  onSend,
  onCancel,
  disabled,
  autoApprove,
  onAutoApproveChange,
}: Props) {
  const [text, setText] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height =
        Math.min(textareaRef.current.scrollHeight, 220) + 'px';
    }
  }, [text]);

  const handleSend = () => {
    if (!text.trim() || disabled) return;
    onSend(text);
    setText('');
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="border-t border-divider px-4 py-3">
      <div className="flex flex-col max-w-3xl mx-auto gap-2">
        <div className="flex items-center gap-2 text-xs text-t-text-muted">
          <label className="inline-flex items-center gap-2 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={autoApprove}
              onChange={(e) => onAutoApproveChange(e.target.checked)}
              disabled={disabled}
              className="rounded border-input-border bg-input-bg text-accent focus:ring-accent"
            />
            自动批准工具调用
          </label>
        </div>
        <div className="card overflow-hidden">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="今天需要什么帮助？"
            disabled={disabled}
            rows={2}
            className="w-full border-none px-4 py-3.5 text-sm resize-none bg-transparent text-t-text placeholder-t-text-muted focus:outline-none disabled:opacity-50"
            style={{ minHeight: '64px', lineHeight: 1.5 }}
          />
          <div className="flex items-center gap-2 px-3 pb-3 pt-0 border-t border-divider">
            <button type="button" className="pill-btn" title="附件" disabled={disabled}>
              <svg viewBox="0 0 24 24">
                <path d="M12 5v14 M5 12h14" />
              </svg>
            </button>
            <div className="flex-1" />
            <button type="button" className="pill-btn" disabled={disabled}>
              <svg viewBox="0 0 24 24">
                <path d="M4 6h16v12H4z M8 6V4h8v2" />
              </svg>
              工作区
            </button>
            <button type="button" className="pill-btn" disabled={disabled}>
              deepseek-v4-pro
              <svg viewBox="0 0 24 24" style={{ width: 12, height: 12 }}>
                <path d="M6 9l6 6 6-6" />
              </svg>
            </button>
            <button
              type="button"
              onClick={handleSend}
              disabled={disabled || !text.trim()}
              className="grid h-10 w-10 flex-shrink-0 place-items-center rounded-full bg-accent text-accent-text shadow-md hover:brightness-105 disabled:opacity-40 disabled:shadow-none"
              title="发送"
            >
              <svg
                viewBox="0 0 24 24"
                className="size-[18px]"
                style={{ stroke: 'currentColor', fill: 'none', strokeWidth: 2 }}
              >
                <path d="M12 19V5M12 5l-6 6M12 5l6 6" />
              </svg>
            </button>
            {disabled && onCancel ? (
              <button
                type="button"
                onClick={onCancel}
                className="px-4 py-2 rounded-lg bg-hover-strong text-t-text text-sm font-medium flex-shrink-0 hover:bg-hover transition-colors"
              >
                停止
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
