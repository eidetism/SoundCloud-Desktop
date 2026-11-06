import { Heart, Pause, Play, Repeat2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useTracks } from '../api/queries';
import { Cover } from '../components/Cover';
import { type PlayerSnapshot, playTracks, togglePlay } from '../player/api';
import { formatTime } from '../player/usePlayer';
import { useRouter } from '../shell/router';

/** A track's page: artwork, title, artist and play. Comments and related tracks come in phase 7. */
export function TrackScreen({ id, player }: { id: number; player: PlayerSnapshot | null }) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const { data, isLoading, error } = useTracks([id]);
  const track = data?.[0];

  if (isLoading) return <p className="text-sm text-muted">{t('player.loading')}</p>;
  if (error || !track) return <p className="text-sm text-danger">{String(error ?? '')}</p>;

  const current = player?.track?.id === track.id;
  const playing = current && player?.status === 'playing';
  const count = (value: number | null) =>
    value === null
      ? '—'
      : new Intl.NumberFormat(i18n.language, { notation: 'compact' }).format(value);

  return (
    <div className="flex gap-7">
      <Cover
        url={track.artworkUrl ?? track.user?.avatarUrl}
        size="t500x500"
        className="h-60 w-60 flex-none rounded-lg"
      />
      <div className="flex min-w-0 flex-col justify-end gap-2 pb-1">
        {track.user && (
          <button
            type="button"
            onClick={() => go({ name: 'artist', id: track.user!.id })}
            className="w-fit text-sm text-muted hover:text-text hover:underline"
          >
            {track.user.username}
          </button>
        )}
        <h1 className="text-[28px] leading-tight font-bold [overflow-wrap:anywhere]">
          {track.title}
        </h1>
        <div className="flex flex-wrap gap-4 text-xs text-muted">
          {track.genre && <span>{track.genre}</span>}
          <span>{formatTime(track.durationMs / 1000)}</span>
          <span className="flex items-center gap-1">
            <Play size={12} /> {count(track.playbackCount)}
          </span>
          <span className="flex items-center gap-1">
            <Heart size={12} /> {count(track.likesCount)}
          </span>
          <span className="flex items-center gap-1">
            <Repeat2 size={12} /> {count(track.repostsCount)}
          </span>
        </div>
        <button
          type="button"
          disabled={track.access === 'blocked'}
          onClick={() => void (current ? togglePlay() : playTracks([track.id], 0))}
          className="mt-3 flex w-fit items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent disabled:opacity-50"
        >
          {playing ? (
            <Pause size={16} fill="currentColor" />
          ) : (
            <Play size={16} fill="currentColor" />
          )}
          {track.access === 'blocked'
            ? t('library.unavailable')
            : playing
              ? t('player.pause')
              : t('player.play')}
        </button>
      </div>
    </div>
  );
}
