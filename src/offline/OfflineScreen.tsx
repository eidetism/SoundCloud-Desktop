import { Download } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { TrackList } from '../components/TrackList';
import type { PlayerSnapshot } from '../player/api';
import { QueryState, ScreenTitle } from '../screens/common';
import { formatBytes, useOfflineSummary, useSavedTracks } from './api';

/** Tracks saved for offline, newest first; they play from disk. */
export function OfflineScreen({ player }: { player: PlayerSnapshot | null }) {
  const { t, i18n } = useTranslation();
  const query = useSavedTracks();
  const summary = useOfflineSummary();
  const tracks = query.data ?? [];
  return (
    <section>
      <ScreenTitle
        extra={
          summary.data &&
          summary.data.savedCount > 0 && (
            <span className="text-xs text-muted">
              {t('settingsOffline.savedValue', {
                count: summary.data.savedCount,
                size: formatBytes(summary.data.savedBytes, i18n.language),
              })}
            </span>
          )
        }
      >
        {t('nav.offline')}
      </ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && tracks.length === 0 && (
        <div className="flex flex-col items-center gap-2 py-16 text-center text-muted">
          <Download size={28} />
          <p className="text-sm">{t('offline.empty')}</p>
          <p className="text-xs">{t('offline.emptyHint')}</p>
        </div>
      )}
      <TrackList tracks={tracks} player={player} />
    </section>
  );
}
