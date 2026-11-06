import {
  LoaderCircle,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  nextTrack,
  type PlayerSnapshot,
  previousTrack,
  type Repeat as RepeatMode,
  seek,
  setRepeat,
  setShuffle,
  setVolume,
  togglePlay,
} from './api';
import { formatTime, usePositionSeconds } from './usePlayer';

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: 'all', all: 'one', one: 'off' };

/** Leaf: the only part that re-renders with the position. */
function Progress({ durationMs }: { durationMs: number }) {
  const position = usePositionSeconds();
  const duration = Math.floor(durationMs / 1000);
  return (
    <div className="flex w-full items-center gap-2 text-[11px] tabular-nums text-[var(--text-muted)]">
      <span className="w-9 text-right">{formatTime(position)}</span>
      <input
        type="range"
        min={0}
        max={duration}
        value={Math.min(position, duration)}
        onChange={(event) => void seek(Number(event.target.value))}
        className="h-1 flex-1 accent-[var(--accent)]"
      />
      <span className="w-9">{formatTime(duration)}</span>
    </div>
  );
}

/** A plain icon button; `active` tints it and adds a dot, like a toggled mode. */
function IconButton({
  label,
  active = false,
  disabled = false,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      onClick={onClick}
      className={`relative grid h-8 w-8 place-items-center rounded-full transition-colors disabled:opacity-40 ${
        active ? 'text-[var(--accent)]' : 'text-[var(--text-muted)] hover:text-[var(--text)]'
      }`}
    >
      {children}
      {active && <span className="absolute bottom-0.5 h-1 w-1 rounded-full bg-[var(--accent)]" />}
    </button>
  );
}

export function PlayerBar({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const { track, status } = player;
  const icon = { size: 18, strokeWidth: 2 };
  const VolumeIcon = player.volume === 0 ? VolumeX : player.volume < 50 ? Volume1 : Volume2;
  const playing = status === 'playing';

  return (
    <footer className="grid grid-cols-[1fr_auto_1fr] items-center gap-4 rounded-2xl border border-[var(--border)] px-4 py-2">
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold">{track?.title ?? t('player.nothing')}</div>
        <div className="truncate text-xs text-[var(--text-muted)]">
          {track?.user?.username}
          {player.preview && ` · ${t('player.preview')}`}
          {player.error && (
            <span className="text-[var(--danger)]"> · {t(`player.error.${player.error}`)}</span>
          )}
        </div>
      </div>

      <div className="flex w-[min(520px,45vw)] flex-col items-center gap-1">
        <div className="flex items-center gap-3">
          <IconButton
            label={t('player.shuffle')}
            active={player.shuffle}
            onClick={() => void setShuffle(!player.shuffle)}
          >
            <Shuffle {...icon} />
          </IconButton>
          <IconButton label={t('player.previous')} onClick={() => void previousTrack()}>
            <SkipBack {...icon} fill="currentColor" />
          </IconButton>
          <button
            type="button"
            title={playing ? t('player.pause') : t('player.play')}
            aria-label={playing ? t('player.pause') : t('player.play')}
            disabled={player.queueLength === 0}
            onClick={() => void togglePlay()}
            className="grid h-9 w-9 place-items-center rounded-full bg-[var(--text)] text-[var(--surface)] transition-transform hover:scale-105 disabled:opacity-40"
          >
            {status === 'loading' ? (
              <LoaderCircle size={18} className="animate-spin" />
            ) : playing ? (
              <Pause size={18} fill="currentColor" />
            ) : (
              <Play size={18} fill="currentColor" className="translate-x-px" />
            )}
          </button>
          <IconButton label={t('player.next')} onClick={() => void nextTrack()}>
            <SkipForward {...icon} fill="currentColor" />
          </IconButton>
          <IconButton
            label={t(`player.repeat.${player.repeat}`)}
            active={player.repeat !== 'off'}
            onClick={() => void setRepeat(NEXT_REPEAT[player.repeat])}
          >
            {player.repeat === 'one' ? <Repeat1 {...icon} /> : <Repeat {...icon} />}
          </IconButton>
        </div>
        {track && <Progress durationMs={track.durationMs} />}
      </div>

      <div className="flex items-center justify-end gap-2">
        <IconButton
          label={t('player.volume')}
          onClick={() => void setVolume(player.volume === 0 ? 60 : 0)}
        >
          <VolumeIcon {...icon} />
        </IconButton>
        <input
          type="range"
          aria-label={t('player.volume')}
          min={0}
          max={100}
          value={player.volume}
          onChange={(event) => void setVolume(Number(event.target.value))}
          className="h-1 w-24 accent-[var(--accent)]"
        />
      </div>
    </footer>
  );
}
