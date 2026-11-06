import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useTracks } from '../api/queries';
import { Cover } from '../components/Cover';
import { type PlayerSnapshot, skipAhead } from './api';
import { formatTime } from './usePlayer';

/** "Up next" popover above the bar's queue button. Click a track to jump to it. */
export function QueuePanel({ player, onClose }: { player: PlayerSnapshot; onClose: () => void }) {
  const { t } = useTranslation();
  const panel = useRef<HTMLDivElement>(null);
  const { data: tracks, isLoading } = useTracks(player.upcoming);

  useEffect(() => {
    const onPointer = (event: PointerEvent) => {
      const button = panel.current?.parentElement;
      if (button && !button.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  // `api_tracks` skips ids SoundCloud no longer knows; keep the step count right.
  const byId = new Map(tracks?.map((track) => [track.id, track]));

  return (
    <div
      ref={panel}
      className="absolute right-0 bottom-[calc(100%+20px)] z-20 flex max-h-[420px] w-[340px] flex-col overflow-hidden rounded-lg bg-side-2 text-side-hi shadow-[0_12px_32px_var(--shade)]"
    >
      <div className="px-4 pt-3.5 pb-2 text-[13px] font-semibold">{t('player.upNext')}</div>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">
        {player.upcoming.length === 0 && (
          <p className="px-4 py-3 text-xs text-side-text">{t('player.queueEmpty')}</p>
        )}
        {isLoading && player.upcoming.length > 0 && (
          <p className="px-4 py-3 text-xs text-side-text">{t('player.loading')}</p>
        )}
        {player.upcoming.map((id, i) => {
          const track = byId.get(id);
          if (!track) return null;
          return (
            <button
              key={`${i}-${id}`}
              type="button"
              onClick={() => void skipAhead(i + 1)}
              className="flex w-full items-center gap-3 px-4 py-1.5 text-left hover:bg-side"
            >
              <Cover
                url={track.artworkUrl ?? track.user?.avatarUrl}
                size="t67x67"
                blank="bg-side"
                className="h-9 w-9 flex-none rounded-md"
              />
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-semibold" title={track.title}>
                  {track.title}
                </div>
                <div className="truncate text-[11px] text-side-text">{track.user?.username}</div>
              </div>
              <span className="text-[11px] text-side-text tabular-nums">
                {formatTime(track.durationMs / 1000)}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
