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
import { type ReactNode, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { scimg } from '../lib/scimg';
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
import { Waveform } from './Waveform';

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: 'all', all: 'one', one: 'off' };

function Control({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      onClick={onClick}
      className={`grid h-7 w-7 place-items-center rounded-full transition-opacity ${
        active === false ? 'opacity-60 hover:opacity-100' : 'opacity-100'
      }`}
    >
      {children}
    </button>
  );
}

/** The sidebar player from the mockup: waveform strip over the accent card. */
export function MiniPlayer({ player }: { player: PlayerSnapshot }) {
  const { t } = useTranslation();
  const [showVolume, setShowVolume] = useState(false);
  const { track, status } = player;
  const playing = status === 'playing';
  const VolumeIcon = player.volume === 0 ? VolumeX : player.volume < 50 ? Volume1 : Volume2;

  const subtitle = player.error
    ? t(`player.error.${player.error}`)
    : [track?.user?.username, player.preview ? t('player.preview') : null]
        .filter(Boolean)
        .join(' · ');

  return (
    <div className="mx-4 mt-auto mb-1">
      <div className="rounded-t-[14px] bg-side-2 px-3.5 pb-1.5">
        <Waveform track={track} playing={playing} />
      </div>
      <div
        className="gradient-accent relative -mt-1.5 rounded-[14px] p-3.5 text-on-accent"
        onWheel={(event) => {
          const step = event.deltaY < 0 ? 5 : -5;
          void setVolume(Math.min(100, Math.max(0, player.volume + step)));
        }}
      >
        <div className="-mt-[38px] mb-2.5 flex items-center justify-center gap-2.5">
          <Control
            label={t('player.shuffle')}
            active={player.shuffle}
            onClick={() => void setShuffle(!player.shuffle)}
          >
            <Shuffle size={14} />
          </Control>
          <Control label={t('player.previous')} onClick={() => void previousTrack()}>
            <SkipBack size={16} fill="currentColor" />
          </Control>
          <button
            type="button"
            title={playing ? t('player.pause') : t('player.play')}
            aria-label={playing ? t('player.pause') : t('player.play')}
            disabled={player.queueLength === 0}
            onClick={() => void togglePlay()}
            className="grid h-11 w-11 place-items-center rounded-full border-[3px] border-side bg-accent transition-transform hover:scale-105 disabled:opacity-60"
          >
            {status === 'loading' ? (
              <LoaderCircle size={16} className="animate-spin" />
            ) : playing ? (
              <Pause size={16} fill="currentColor" />
            ) : (
              <Play size={16} fill="currentColor" className="translate-x-px" />
            )}
          </button>
          <Control label={t('player.next')} onClick={() => void nextTrack()}>
            <SkipForward size={16} fill="currentColor" />
          </Control>
          <Control
            label={t(`player.repeat.${player.repeat}`)}
            active={player.repeat !== 'off'}
            onClick={() => void setRepeat(NEXT_REPEAT[player.repeat])}
          >
            {player.repeat === 'one' ? <Repeat1 size={14} /> : <Repeat size={14} />}
          </Control>
        </div>

        {showVolume ? (
          <div
            className="flex h-[38px] items-center gap-2"
            onMouseLeave={() => setShowVolume(false)}
          >
            <Control
              label={t('player.volume')}
              onClick={() => void setVolume(player.volume === 0 ? 60 : 0)}
            >
              <VolumeIcon size={16} />
            </Control>
            <input
              type="range"
              aria-label={t('player.volume')}
              min={0}
              max={100}
              value={player.volume}
              onChange={(event) => void setVolume(Number(event.target.value))}
              className="h-1 min-w-0 flex-1 [accent-color:var(--on-accent)]"
            />
          </div>
        ) : (
          <div className="flex items-center gap-2.5 text-xs">
            {track?.artworkUrl || track?.user?.avatarUrl ? (
              <img
                src={scimg(track.artworkUrl ?? track.user?.avatarUrl, 't67x67')}
                alt=""
                loading="lazy"
                decoding="async"
                className="h-[38px] w-[38px] flex-none rounded-lg object-cover"
              />
            ) : (
              <div className="h-[38px] w-[38px] flex-none rounded-lg bg-shade" />
            )}
            <div className="min-w-0 flex-1">
              <div className="truncate font-semibold">{track?.title ?? t('player.nothing')}</div>
              <small className="mt-0.5 block truncate text-[10px] opacity-80">{subtitle}</small>
            </div>
            <Control label={t('player.volume')} active={false} onClick={() => setShowVolume(true)}>
              <VolumeIcon size={15} />
            </Control>
          </div>
        )}
      </div>
    </div>
  );
}
