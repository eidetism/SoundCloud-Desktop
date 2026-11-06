import { useVirtualizer } from '@tanstack/react-virtual';
import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { playTracks, type Track } from '../player/api';
import { formatTime } from '../player/usePlayer';

interface Page<T> {
  collection: T[];
  nextHref: string | null;
}

interface TrackLike {
  createdAt: string;
  track: Track;
}

const ROW_HEIGHT = 44;

/** The signed-in user's liked tracks, paged and virtualized. Click plays from there. */
export function LikesList({ currentId }: { currentId: number | null }) {
  const { t } = useTranslation();
  const [tracks, setTracks] = useState<Track[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);

  const loadMore = useCallback(async () => {
    if (loading || cursor === null) return;
    setLoading(true);
    setError(null);
    try {
      const page = await invoke<Page<TrackLike>>('api_likes', { cursor: cursor ?? null });
      setTracks((previous) => [...previous, ...page.collection.map((like) => like.track)]);
      setCursor(page.nextHref);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, [cursor, loading]);

  useEffect(() => {
    if (cursor === undefined) void loadMore();
  }, [cursor, loadMore]);

  const rows = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });
  const items = rows.getVirtualItems();
  const last = items[items.length - 1]?.index ?? 0;

  useEffect(() => {
    if (tracks.length > 0 && last >= tracks.length - 10) void loadMore();
  }, [last, tracks.length, loadMore]);

  const ids = tracks.map((track) => track.id);

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-2">
      <h2 className="text-lg font-semibold">{t('library.likes')}</h2>
      {error && <p className="text-sm text-[var(--danger)]">{error}</p>}
      <div ref={scroller} className="min-h-0 flex-1 overflow-auto">
        <div className="relative" style={{ height: rows.getTotalSize() }}>
          {items.map((row) => {
            const track = tracks[row.index];
            const blocked = track.access === 'blocked';
            return (
              <button
                key={track.id}
                type="button"
                disabled={blocked}
                onDoubleClick={() => void playTracks(ids, row.index)}
                className="absolute left-0 flex w-full items-center gap-3 rounded-lg px-3 text-left hover:bg-[var(--border)] disabled:opacity-40"
                style={{ height: ROW_HEIGHT, transform: `translateY(${row.start}px)` }}
              >
                <span
                  className={`min-w-0 flex-1 truncate ${track.id === currentId ? 'text-[var(--accent)]' : ''}`}
                >
                  {track.title}
                </span>
                <span className="w-40 truncate text-sm text-[var(--text-muted)]">
                  {track.user?.username}
                </span>
                <span className="w-24 text-right text-xs text-[var(--text-muted)]">
                  {blocked
                    ? t('library.unavailable')
                    : track.access === 'preview'
                      ? t('player.preview')
                      : formatTime(track.durationMs / 1000)}
                </span>
              </button>
            );
          })}
        </div>
        {loading && <p className="p-2 text-sm text-[var(--text-muted)]">{t('player.loading')}</p>}
      </div>
    </section>
  );
}
