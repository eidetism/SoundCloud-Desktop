import { LoaderCircle } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

/** A screen's heading row. */
export function ScreenTitle({ children, extra }: { children: ReactNode; extra?: ReactNode }) {
  return (
    <div className="mb-3 flex items-center justify-between gap-4">
      <h1 className="truncate text-xl font-bold">{children}</h1>
      {extra}
    </div>
  );
}

/** Loading spinner, or the error with a retry, for a query that has no data yet. */
export function QueryState({
  loading,
  error,
  onRetry,
}: {
  loading: boolean;
  error: unknown;
  onRetry?: () => void;
}) {
  const { t } = useTranslation();
  if (error) {
    return (
      <div className="flex items-center gap-3 py-6 text-sm text-muted">
        <span>{t('common.loadFailed')}</span>
        {onRetry && (
          <button type="button" onClick={onRetry} className="text-accent hover:underline">
            {t('common.retry')}
          </button>
        )}
      </div>
    );
  }
  if (loading) {
    return (
      <div className="grid place-items-center py-10 text-muted">
        <LoaderCircle size={22} className="animate-spin" />
      </div>
    );
  }
  return null;
}

/** "Show more" under a paged list. */
export function MoreButton({
  visible,
  busy,
  onClick,
}: {
  visible: boolean;
  busy: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  if (!visible) return null;
  return (
    <button
      type="button"
      disabled={busy}
      onClick={onClick}
      className="mx-auto mt-4 block rounded-full border border-line px-4 py-1.5 text-xs text-muted hover:text-text disabled:opacity-50"
    >
      {busy ? t('player.loading') : t('common.more')}
    </button>
  );
}

export const compact = (value: number, language: string) =>
  new Intl.NumberFormat(language, { notation: 'compact' }).format(value);
