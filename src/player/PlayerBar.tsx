import {
  Heart,
  ListMusic,
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
import { type ReactNode, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useLikedIds, useSetLike } from '../api/queries';
import { Cover } from '../components/Cover';
import { useRouter } from '../shell/router';
import {
  nextTrack,
  type PlayerSnapshot,
  previousTrack,
  type Repeat as RepeatMode,
  setRepeat,
  setShuffle,
  setVolume,
  togglePlay,
} from './api';
import { QueuePanel } from './QueuePanel';
import { formatTime, usePositionSeconds } from './usePlayer';
import { Waveform } from './Waveform';

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: 'all', all: 'one', one: 'off' };

function IconButton({
  label,
  active,
  open,
  disabled,
  onClick,
  children,
}: {
  label: string;
  /** A playback mode that is on (shuffle, repeat): accent. */
  active?: boolean;
  /** A panel this button opened: white, the accent stays for playback. */
  open?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
}) {
  const color = active
    ? 'text-accent'
    : open
      ? 'text-side-hi'
      : 'text-side-text enabled:hover:text-side-hi';
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active ?? open}
      disabled={disabled}
      onClick={onClick}
      className={`grid h-8 w-8 flex-none place-items-center rounded-full transition-colors disabled:cursor-default ${color}`}
    >
      {children}
    </button>
  );
}

/** Played time; the only part of the bar that re-renders every second. */
function Elapsed() {
  return <>{formatTime(usePositionSeconds())}</>;
}

function NowPlaying({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const liked = useLikedIds().data;
  const setLike = useSetLike();
  const { track } = player;
  const isLiked = track ? (liked?.has(track.id) ?? false) : false;

  const subtitle = player.error
    ? t(`player.error.${player.error}`)
    : [track?.user?.username, player.preview ? t('player.preview') : null]
        .filter(Boolean)
        .join(' · ');

  return (
    <div className="flex min-w-0 items-center gap-3">
      <Cover
        url={track?.artworkUrl ?? track?.user?.avatarUrl}
        size="t67x67"
        blank="bg-side-2"
        className="h-12 w-12 flex-none rounded-lg"
      />
      <div className="min-w-0 flex-1">
        {track ? (
          <button
            type="button"
            title={track.title}
            onClick={() => go({ name: 'track', id: track.id })}
            className="block max-w-full truncate text-left text-[13px] font-semibold text-side-hi hover:underline"
          >
            {track.title}
          </button>
        ) : (
          <div className="truncate text-[13px] font-semibold text-side-hi">
            {t('player.nothing')}
          </div>
        )}
        <div className="mt-0.5 truncate text-xs text-side-text" title={subtitle}>
          {subtitle}
        </div>
      </div>
      {track && (
        <IconButton
          label={isLiked ? t('player.unlike') : t('player.like')}
          open={isLiked}
          disabled={liked === undefined}
          onClick={() => setLike.mutate({ id: track.id, liked: !isLiked })}
        >
          <Heart size={16} fill={isLiked ? 'currentColor' : 'none'} />
        </IconButton>
      )}
    </div>
  );
}

function Transport({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const { track, status } = player;
  const playing = status === 'playing';
  const empty = player.queueLength === 0;

  return (
    <div className="flex min-w-0 flex-col items-center gap-1">
      <div className="flex items-center gap-3">
        <IconButton
          label={t('player.shuffle')}
          active={player.shuffle}
          onClick={() => void setShuffle(!player.shuffle)}
        >
          <Shuffle size={16} />
        </IconButton>
        <IconButton
          label={t('player.previous')}
          disabled={empty}
          onClick={() => void previousTrack()}
        >
          <SkipBack size={18} fill="currentColor" />
        </IconButton>
        <button
          type="button"
          title={playing ? t('player.pause') : t('player.play')}
          aria-label={playing ? t('player.pause') : t('player.play')}
          disabled={empty}
          onClick={() => void togglePlay()}
          className="grid h-10 w-10 place-items-center rounded-full bg-accent text-on-accent transition-transform enabled:hover:scale-105 disabled:opacity-50"
        >
          {status === 'loading' ? (
            <LoaderCircle size={18} className="animate-spin" />
          ) : playing ? (
            <Pause size={18} fill="currentColor" />
          ) : (
            <Play size={18} fill="currentColor" className="translate-x-px" />
          )}
        </button>
        <IconButton label={t('player.next')} disabled={empty} onClick={() => void nextTrack()}>
          <SkipForward size={18} fill="currentColor" />
        </IconButton>
        <IconButton
          label={t(`player.repeat.${player.repeat}`)}
          active={player.repeat !== 'off'}
          onClick={() => void setRepeat(NEXT_REPEAT[player.repeat])}
        >
          {player.repeat === 'one' ? <Repeat1 size={16} /> : <Repeat size={16} />}
        </IconButton>
      </div>
      <div className="flex w-full max-w-[640px] items-center gap-3 text-[11px] text-side-text tabular-nums">
        <span className="w-10 flex-none text-right">{track ? <Elapsed /> : '0:00'}</span>
        <Waveform track={track} playing={playing} />
        <span className="w-10 flex-none">{formatTime((track?.durationMs ?? 0) / 1000)}</span>
      </div>
    </div>
  );
}

function VolumeAndQueue({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const [queueOpen, setQueueOpen] = useState(false);
  const beforeMute = useRef(60);
  const { volume } = player;
  const VolumeIcon = volume === 0 ? VolumeX : volume < 50 ? Volume1 : Volume2;

  return (
    <div
      className="relative flex items-center justify-end gap-2"
      onWheel={(event) => {
        const step = event.deltaY < 0 ? 5 : -5;
        void setVolume(Math.min(100, Math.max(0, volume + step)));
      }}
    >
      <IconButton
        label={t('player.queue')}
        open={queueOpen}
        onClick={() => setQueueOpen((open) => !open)}
      >
        <ListMusic size={18} />
      </IconButton>
      <IconButton
        label={volume === 0 ? t('player.unmute') : t('player.mute')}
        onClick={() => {
          if (volume > 0) beforeMute.current = volume;
          void setVolume(volume === 0 ? beforeMute.current : 0);
        }}
      >
        <VolumeIcon size={18} />
      </IconButton>
      <input
        type="range"
        aria-label={t('player.volume')}
        title={`${t('player.volume')}: ${Math.round(volume)}%`}
        min={0}
        max={100}
        value={volume}
        onChange={(event) => void setVolume(Number(event.target.value))}
        style={{ '--fill': `${volume}%` } as React.CSSProperties}
        className="bar-range w-[100px] flex-none"
      />
      {queueOpen && <QueuePanel player={player} onClose={() => setQueueOpen(false)} />}
    </div>
  );
}

/** The bottom bar under the sidebar and content, full window width. */
export function PlayerBar({ player }: { player: PlayerSnapshot }) {
  return (
    <footer className="mx-3 mb-3 grid h-20 flex-none grid-cols-[280px_minmax(0,1fr)_240px] items-center gap-6 rounded-lg bg-side px-4">
      <NowPlaying player={player} />
      <Transport player={player} />
      <VolumeAndQueue player={player} />
    </footer>
  );
}
