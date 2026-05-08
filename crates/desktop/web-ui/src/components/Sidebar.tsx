interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

interface Props {
  sessions: SessionInfo[];
  isOpen: boolean;
  onToggle: () => void;
  onNewSession: () => void;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
}

export default function Sidebar({
  sessions,
  isOpen,
  onToggle,
  onNewSession,
  onSelectSession,
  onDeleteSession,
}: Props) {
  return (
    <>
      {!isOpen && (
        <button
          onClick={onToggle}
          className="absolute left-2 top-2 z-10 p-2 text-gray-400 hover:text-gray-200 bg-gray-800 rounded-lg"
          title="打开会话列表"
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M3 12h18M3 6h18M3 18h18" />
          </svg>
        </button>
      )}

      <div
        className={`${isOpen ? 'w-64' : 'w-0'} bg-gray-900 border-r border-gray-800
                    flex flex-col transition-all duration-200 overflow-hidden`}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-800">
          <span className="text-sm font-semibold text-gray-200">
            📁 会话
          </span>
          <button
            onClick={onToggle}
            className="text-gray-500 hover:text-gray-300"
          >
            ✕
          </button>
        </div>

        <button
          onClick={onNewSession}
          className="mx-3 mt-3 px-3 py-2 text-sm text-indigo-400 hover:bg-gray-800 rounded-lg text-left"
        >
          + 新会话
        </button>

        <div className="flex-1 overflow-y-auto px-2 py-2">
          {sessions.length === 0 && (
            <p className="text-xs text-gray-600 px-2 py-4 text-center">
              暂无会话
            </p>
          )}
          {sessions.map((s) => (
            <div
              key={s.id}
              className="flex items-center gap-1 rounded-lg hover:bg-gray-800 group"
            >
              <button
                type="button"
                onClick={() => onSelectSession?.(s.id)}
                className="flex-1 min-w-0 px-3 py-2 text-sm text-gray-400 text-left truncate"
              >
                {s.name || s.id.slice(0, 8)}
              </button>
              {onDeleteSession && (
                <button
                  type="button"
                  title="删除会话"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteSession(s.id);
                  }}
                  className="shrink-0 px-2 py-2 text-gray-600 hover:text-red-400 opacity-60 group-hover:opacity-100"
                >
                  ×
                </button>
              )}
            </div>
          ))}
        </div>

        <div className="px-4 py-3 border-t border-gray-800">
          <p className="text-xs text-gray-600">DeepSeek Desktop v0.1</p>
        </div>
      </div>
    </>
  );
}
