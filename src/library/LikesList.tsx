import { useVirtualizer } from '@tanstack/react-virtual';
import { invoke } from '@tauri-apps/api/core';
import { Pause, Play } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { scimg } from '../lib/scimg';
import { type PlayerSnapshot, playTracks, type Track, togglePlay } from '../player/api';
import { formatTime } from '../player/usePlayer';

interface Page<T> {
  collection: T[];
  nextHref: string | null;
}

interface TrackLike {
  createdAt: string;
  track: Track;
}

const ROW_HEIGHT = 58;

/** The signed-in user's liked tracks, paged and virtualized, in the mockup's row style. */
export function LikesList({ player }: { player: PlayerSnapshot | null }) {
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
    overscan: 6,
  });
  const items = rows.getVirtualItems();
  const last = items[items.length - 1]?.index ?? 0;

  useEffect(() => {
    if (tracks.length > 0 && last >= tracks.length - 10) void loadMore();
  }, [last, tracks.length, loadMore]);

  const ids = tracks.map((track) => track.id);
  const currentId = player?.track?.id ?? null;
  const playing = player?.status === 'playing' || player?.status === 'loading';

  return (
    <section className="flex h-full min-h-0 flex-col">
      <div className="mb-3 flex items-baseline justify-between">
        <h1 className="text-[15px] font-bold">{t('nav.likes')}</h1>
        {error && <p className="text-xs text-danger">{error}</p>}
      </div>
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
        <div className="relative" style={{ height: rows.getTotalSize() }}>
          {items.map((row) => {
            const track = tracks[row.index];
            const blocked = track.access === 'blocked';
            const current = track.id === currentId;
            const play = () => {
              if (current) void togglePlay();
              else void playTracks(ids, row.index);
            };
            return (
              <div
                key={track.id}
                onDoubleClick={() => !blocked && void playTracks(ids, row.index)}
                className={`absolute left-0 grid w-full grid-cols-[24px_44px_1fr_auto_30px] items-center gap-3 rounded-md pr-2 text-[13px] hover:bg-hover ${
                  blocked ? 'opacity-45' : ''
                }`}
                style={{ height: ROW_HEIGHT, transform: `translateY(${row.start}px)` }}
              >
                <span className="text-right text-[11px] text-muted tabular-nums">
                  {String(row.index + 1).padStart(2, '0')}
                </span>
                {track.artworkUrl || track.user?.avatarUrl ? (
                  <img
                    src={scimg(track.artworkUrl ?? track.user?.avatarUrl, 't67x67')}
                    alt=""
                    loading="lazy"
                    decoding="async"
                    className="h-11 w-11 rounded-lg bg-card object-cover"
                  />
                ) : (
                  <div className="h-11 w-11 rounded-lg bg-card" />
                )}
                <div className="min-w-0">
                  <div className={`truncate font-semibold ${current ? 'text-accent' : ''}`}>
                    {track.title}
                  </div>
                  <small className="mt-[3px] block truncate text-[11px] text-muted">
                    {track.user?.username}
                    {track.genre ? ` · ${track.genre}` : ''}
                  </small>
                </div>
                <span className="text-xs text-muted tabular-nums">
                  {blocked
                    ? t('library.unavailable')
                    : track.access === 'preview'
                      ? t('player.preview')
                      : formatTime(track.durationMs / 1000)}
                </span>
                <button
                  type="button"
                  disabled={blocked}
                  title={current && playing ? t('player.pause') : t('player.play')}
                  onClick={play}
                  className="grid h-[26px] w-[26px] place-items-center rounded-full bg-accent text-on-accent disabled:invisible"
                >
                  {current && playing ? (
                    <Pause size={10} fill="currentColor" />
                  ) : (
                    <Play size={10} fill="currentColor" className="translate-x-px" />
                  )}
                </button>
              </div>
            );
          })}
        </div>
        {loading && <p className="p-2 text-xs text-muted">{t('player.loading')}</p>}
      </div>
    </section>
  );
}
