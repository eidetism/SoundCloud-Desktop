import { useTranslation } from 'react-i18next';
import {
  nextTrack,
  type PlayerSnapshot,
  previousTrack,
  type Repeat,
  seek,
  setRepeat,
  setShuffle,
  setVolume,
  togglePlay,
} from './api';
import { formatTime, usePositionSeconds } from './usePlayer';

const NEXT_REPEAT: Record<Repeat, Repeat> = { off: 'all', all: 'one', one: 'off' };

/** Leaf: the only part that re-renders with the position. */
function Progress({ durationMs }: { durationMs: number }) {
  const position = usePositionSeconds();
  const duration = Math.floor(durationMs / 1000);
  return (
    <div className="flex items-center gap-2 text-xs tabular-nums text-[var(--text-muted)]">
      <span className="w-10 text-right">{formatTime(position)}</span>
      <input
        type="range"
        min={0}
        max={duration}
        value={Math.min(position, duration)}
        onChange={(event) => void seek(Number(event.target.value))}
        className="flex-1 accent-[var(--accent)]"
      />
      <span className="w-10">{formatTime(duration)}</span>
    </div>
  );
}

export function PlayerBar({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const { track, status } = player;
  const button = 'h-8 min-w-8 rounded-full px-2 hover:bg-[var(--border)] disabled:opacity-40';

  return (
    <footer className="flex flex-col gap-2 rounded-2xl border border-[var(--border)] p-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="truncate font-semibold">{track?.title ?? t('player.nothing')}</div>
          <div className="truncate text-sm text-[var(--text-muted)]">
            {track?.user?.username}
            {player.preview && ` · ${t('player.preview')}`}
            {status === 'loading' && ` · ${t('player.loading')}`}
            {player.error && (
              <span className="text-[var(--danger)]"> · {t(`player.error.${player.error}`)}</span>
            )}
          </div>
        </div>
        <button
          type="button"
          className={button}
          aria-pressed={player.shuffle}
          onClick={() => void setShuffle(!player.shuffle)}
        >
          <span className={player.shuffle ? 'text-[var(--accent)]' : ''}>
            {t('player.shuffle')}
          </span>
        </button>
        <button type="button" className={button} onClick={() => void previousTrack()}>
          ⏮
        </button>
        <button
          type="button"
          className={`${button} bg-[var(--accent)] text-white hover:bg-[var(--accent)]`}
          disabled={player.queueLength === 0}
          onClick={() => void togglePlay()}
        >
          {status === 'playing' ? '⏸' : '▶'}
        </button>
        <button type="button" className={button} onClick={() => void nextTrack()}>
          ⏭
        </button>
        <button
          type="button"
          className={button}
          onClick={() => void setRepeat(NEXT_REPEAT[player.repeat])}
        >
          <span className={player.repeat !== 'off' ? 'text-[var(--accent)]' : ''}>
            {t(`player.repeat.${player.repeat}`)}
          </span>
        </button>
        <input
          type="range"
          aria-label={t('player.volume')}
          min={0}
          max={100}
          value={player.volume}
          onChange={(event) => void setVolume(Number(event.target.value))}
          className="w-24 accent-[var(--accent)]"
        />
      </div>
      {track && <Progress durationMs={track.durationMs} />}
    </footer>
  );
}
