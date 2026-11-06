import { Play } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { usePlaylist, useTracks } from '../api/queries';
import type { SystemPlaylist, Track } from '../api/types';
import { Cover } from '../components/Cover';
import { TrackList } from '../components/TrackList';
import { type PlayerSnapshot, playTracks } from '../player/api';
import { formatTime } from '../player/usePlayer';
import { useRouter } from '../shell/router';
import { QueryState } from './common';

/** Header shared by playlists, albums and mixes. */
function CollectionHeader({
  artwork,
  kind,
  title,
  by,
  tracks,
}: {
  artwork: string | null | undefined;
  kind: string;
  title: string;
  by?: ReactNode;
  tracks: Track[];
}) {
  const { t } = useTranslation();
  const playable = tracks.filter((track) => track.access !== 'blocked');
  const seconds = tracks.reduce((sum, track) => sum + track.durationMs / 1000, 0);
  return (
    <div className="mb-6 flex gap-6">
      <Cover url={artwork} size="t500x500" className="h-48 w-48 flex-none rounded-lg" />
      <div className="flex min-w-0 flex-col justify-end gap-2 pb-1">
        <div className="text-xs font-semibold text-muted uppercase">{kind}</div>
        <h1 className="text-[28px] leading-tight font-bold [overflow-wrap:anywhere]">{title}</h1>
        <div className="text-xs text-muted">
          {by}
          {by ? ' · ' : ''}
          {t('collection.tracks', { count: tracks.length })}
          {seconds > 0 ? ` · ${formatTime(seconds)}` : ''}
        </div>
        <button
          type="button"
          disabled={playable.length === 0}
          onClick={() =>
            void playTracks(
              tracks.map((track) => track.id),
              0,
            )
          }
          className="mt-3 flex w-fit items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent disabled:opacity-50"
        >
          <Play size={16} fill="currentColor" />
          {t('player.play')}
        </button>
      </div>
    </div>
  );
}

function ArtistLink({ id, name }: { id: number; name: string }) {
  const go = useRouter((state) => state.go);
  return (
    <button
      type="button"
      onClick={() => go({ name: 'artist', id })}
      className="font-semibold text-text hover:underline"
    >
      {name}
    </button>
  );
}

export function PlaylistScreen({
  id,
  secretToken,
  player,
}: {
  id: number;
  secretToken: string | null;
  player: PlayerSnapshot | null;
}) {
  const { t } = useTranslation();
  const playlist = usePlaylist(id, secretToken);
  const tracks = useTracks(playlist.data?.trackIds ?? []);
  const data = playlist.data;
  return (
    <section>
      <QueryState
        loading={playlist.isLoading}
        error={playlist.error}
        onRetry={() => void playlist.refetch()}
      />
      {data && (
        <>
          <CollectionHeader
            artwork={data.artworkUrl ?? data.user?.avatarUrl}
            kind={data.isAlbum ? t('collection.album') : t('collection.playlist')}
            title={data.title}
            by={data.user ? <ArtistLink id={data.user.id} name={data.user.username} /> : undefined}
            tracks={tracks.data ?? []}
          />
          <QueryState
            loading={tracks.isLoading}
            error={tracks.error}
            onRetry={() => void tracks.refetch()}
          />
          <TrackList tracks={tracks.data ?? []} player={player} />
        </>
      )}
    </section>
  );
}

/** A generated playlist (Weekly Wave, Your Mix, a station…) from `/mixed-selections`. */
export function MixScreen({ mix, player }: { mix: SystemPlaylist; player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const tracks = useTracks(mix.trackIds);
  return (
    <section>
      <CollectionHeader
        artwork={mix.calculatedArtworkUrl}
        kind={t('collection.mix')}
        title={mix.title}
        by={mix.shortDescription ?? undefined}
        tracks={tracks.data ?? []}
      />
      <QueryState
        loading={tracks.isLoading}
        error={tracks.error}
        onRetry={() => void tracks.refetch()}
      />
      <TrackList tracks={tracks.data ?? []} player={player} />
    </section>
  );
}
