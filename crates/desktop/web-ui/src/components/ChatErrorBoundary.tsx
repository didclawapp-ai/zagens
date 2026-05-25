import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  onReset?: () => void;
}

interface State {
  error: Error | null;
}

/** Prevents one bad message render from blanking the whole transcript column. */
export class ChatErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[zagens] chat render error', error, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.error) {
      return (
        <div className="my-4 rounded-lg border border-t-error/30 bg-error-bg px-4 py-3 text-sm text-t-error">
          <p className="font-medium">消息渲染失败</p>
          <p className="mt-1 text-xs opacity-90 break-words">{this.state.error.message}</p>
          <button
            type="button"
            className="mt-2 text-xs underline"
            onClick={() => {
              this.setState({ error: null });
              this.props.onReset?.();
            }}
          >
            重试显示
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
