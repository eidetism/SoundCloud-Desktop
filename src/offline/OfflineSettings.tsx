import { FolderOpen } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  formatBytes,
  onRelocateProgress,
  useClearRecent,
  useOfflineSummary,
  useRelocate,
  useSetRecentLimit,
} from './api';

const LIMITS_MB = [0, 512, 1024, 2048, 5120, 10240];
const KNOWN_ERRORS = ['same', 'nested', 'not_writable'];

/** Settings → Offline: the folder (and moving it), space used, recent tracks. */
export function OfflineSettings() {
  const { t, i18n } = useTranslation();
  const summary = useOfflineSummary();
  const relocate = useRelocate();
  const setLimit = useSetRecentLimit();
  const clear = useClearRecent();
  const [percent, setPercent] = useState<number | null>(null);

  useEffect(() => {
    if (!relocate.isPending) {
      setPercent(null);
      return;
    }
    const stop = onRelocateProgress((done, total) =>
      setPercent(total > 0 ? Math.floor((done / total) * 100) : 100),
    );
    return () => void stop.then((unlisten) => unlisten());
  }, [relocate.isPending]);

  const data = summary.data;
  if (!data) return null;
  const size = (bytes: number) => formatBytes(bytes, i18n.language);
  const limitLabel = (mb: number) => (mb === 0 ? t('settingsOffline.off') : size(mb * 1024 * 1024));
  const relocateError = relocate.error ? String(relocate.error) : null;

  return (
    <section className="flex flex-col divide-y divide-line">
      <h2 className="pb-2 text-base font-bold">{t('settingsOffline.title')}</h2>
      <div className="flex flex-col gap-2 py-3">
        <div className="flex items-center justify-between gap-6">
          <span className="text-sm font-semibold">{t('settingsOffline.folder')}</span>
          <div className="flex gap-2">
            {!data.defaultFolder && (
              <button
                type="button"
                disabled={relocate.isPending}
                onClick={() => relocate.mutate(null)}
                className="rounded-full px-3 py-1.5 text-xs font-semibold text-muted hover:text-text disabled:opacity-50"
              >
                {t('settingsOffline.reset')}
              </button>
            )}
            <button
              type="button"
              disabled={relocate.isPending}
              onClick={() => relocate.mutate('pick')}
              className="flex items-center gap-1.5 rounded-full bg-card px-3.5 py-1.5 text-xs font-semibold hover:bg-hover disabled:opacity-50"
            >
              <FolderOpen size={14} />
              {t('settingsOffline.change')}
            </button>
          </div>
        </div>
        <span className="text-xs break-all text-muted">{data.folder}</span>
        {percent !== null && (
          <span className="text-xs text-accent">{t('settingsOffline.moving', { percent })}</span>
        )}
        {relocateError && (
          <span className="text-xs text-danger">
            {KNOWN_ERRORS.includes(relocateError)
              ? t(`settingsOffline.errors.${relocateError}`)
              : t('settingsOffline.errors.other', { message: relocateError })}
          </span>
        )}
      </div>
      <div className="flex items-center justify-between gap-6 py-3">
        <span className="text-sm font-semibold">{t('settingsOffline.saved')}</span>
        <span className="text-xs text-muted">
          {t('settingsOffline.savedValue', { count: data.savedCount, size: size(data.savedBytes) })}
        </span>
      </div>
      <div className="flex flex-col gap-2 py-3">
        <div className="flex items-center justify-between gap-6">
          <span className="text-sm font-semibold">{t('settingsOffline.recent')}</span>
          <select
            value={data.recentLimitMb}
            disabled={setLimit.isPending}
            onChange={(event) => setLimit.mutate(Number(event.target.value))}
            className="rounded-full bg-card px-3 py-1.5 text-xs font-semibold outline-none"
          >
            {[...new Set([...LIMITS_MB, data.recentLimitMb])]
              .sort((a, b) => a - b)
              .map((mb) => (
                <option key={mb} value={mb}>
                  {limitLabel(mb)}
                </option>
              ))}
          </select>
        </div>
        <span className="text-xs text-muted">{t('settingsOffline.recentHint')}</span>
        {data.recentCount > 0 && (
          <div className="flex items-center justify-between gap-6">
            <span className="text-xs text-muted">
              {t('settingsOffline.recentUsed', {
                size: size(data.recentBytes),
                count: data.recentCount,
              })}
            </span>
            <button
              type="button"
              disabled={clear.isPending}
              onClick={() => clear.mutate(undefined)}
              className="rounded-full px-3 py-1.5 text-xs font-semibold text-danger hover:bg-hover disabled:opacity-50"
            >
              {t('settingsOffline.clear')}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
