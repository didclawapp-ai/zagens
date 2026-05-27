import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useT } from '../i18n';

interface Props {
  children: ReactNode;
  onReset?: () => void;
}

interface State {
  error: Error | null;
}

function ChatErrorFallback({
  error,
  onReset,
}: {
  error: Error;
  onReset: () => void;
}) {
  const { t } = useT();
  return (
    <div className="my-4 rounded-lg border border-t-error/30 bg-error-bg px-4 py-3 text-sm text-t-error">
      <p className="font-medium">{t('chatError.renderFailed')}</p>
      <p className="mt-1 text-xs opacity-90 break-words">{error.message}</p>
      <button type="button" className="mt-2 text-xs underline" onClick={onReset}>
        {t('chatError.retryDisplay')}
      </button>
    </div>
  );
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
        <ChatErrorFallback
          error={this.state.error}
          onReset={() => {
            this.setState({ error: null });
            this.props.onReset?.();
          }}
        />
      );
    }
    return this.props.children;
  }
}
