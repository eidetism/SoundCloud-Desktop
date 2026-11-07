import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { flatten } from '../../api/queries';
import { useLikers, useRelated, useTrackSets } from '../../api/track';
import type { Playlist, Track } from '../../api/types';
import { Cover } from '../../components/Cover';
import { type PlayerSnapshot, playTracks, togglePlay } from '../../player/api';
import { formatTime } from '../../player/usePlayer';
import { useRouter } from '../../shell/router';
import { compact } from '../common';

const RELATED_SHOWN = 5;
const SETS_SHOWN = 3;
const LIKER_AVATARS = 8;

function Block({
  title,
  extra,
  onAll,
  allLabel,
  children,
}: {
  title: string;
  extra?: string;
  onAll?: () => void;
  allLabel?: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <div className="mb-2 flex items-baseline justify-between">
        <b className="text-[14px]">{title}</b>
        {onAll ? (
          <button type="button" onClick={onAll} className="text-[11px] text-muted hover:text-text">
            {allLabel}
          </button>
        ) : (
          extra && <span className="text-[11px] text-muted">{extra}</span>
        )}
      </div>
      {children}
    </section>
  );
}

/** Rows standing in for a block's content while it loads. */
function Skeleton({ rows, round = false }: { rows: number; round?: boolean }) {
  return (
    <div className="flex flex-col gap-2">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-center gap-2.5">
          <div className={`h-10 w-10 flex-none bg-card ${round ? 'rounded-full' : 'rounded-md'}`} />
          <div className="flex-1">
            <div className="h-2.5 w-3/4 rounded bg-card" />
            <div className="mt-1.5 h-2 w-1/2 rounded bg-card" />
          </div>
        </div>
      ))}
    </div>
  );
}

function Failed({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <p className="text-[11px] text-muted">
      {t('common.loadFailed')} ·{' '}
      <button type="button" onClick={onRetry} className="text-accent hover:underline">
        {t('common.retry')}
      </button>
    </p>
  );
}

function Related({ id, player }: { id: number; player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const query = useRelated(id);
  const [all, setAll] = useState(false);
  const tracks = query.data ?? [];
  if (query.isSuccess && tracks.length === 0) return null;
  const shown = all ? tracks : tracks.slice(0, RELATED_SHOWN);
  const ids = tracks.map((track) => track.id);

  return (
    <Block
      title={t('trackPage.related')}
      onAll={tracks.length > RELATED_SHOWN ? () => setAll((value) => !value) : undefined}
      allLabel={all ? t('trackPage.collapse') : t('common.all')}
    >
      {query.isLoading && <Skeleton rows={RELATED_SHOWN} />}
      {query.isError && <Failed onRetry={() => void query.refetch()} />}
      <div className="flex flex-col">
        {shown.map((track, index) => (
          <RelatedRow
            key={track.id}
            track={track}
            current={player?.track?.id === track.id}
            onPlay={() =>
              player?.track?.id === track.id ? void togglePlay() : void playTracks(ids, index)
            }
            onOpen={() => go({ name: 'track', id: track.id })}
          />
        ))}
      </div>
    </Block>
  );
}

function RelatedRow({
  track,
  current,
  onPlay,
  onOpen,
}: {
  track: Track;
  current: boolean;
  onPlay: () => void;
  onOpen: () => void;
}) {
  const blocked = track.access === 'blocked';
  return (
    <div
      onDoubleClick={() => !blocked && onPlay()}
      className={`flex items-center gap-2.5 rounded-md py-1 pr-1 hover:bg-hover ${blocked ? 'opacity-45' : ''}`}
    >
      <button type="button" disabled={blocked} onClick={onPlay} className="flex-none">
        <Cover
          url={track.artworkUrl ?? track.user?.avatarUrl}
          size="t67x67"
          className="h-10 w-10 rounded-md"
        />
      </button>
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={onOpen}
          title={track.title}
          className={`block max-w-full truncate text-left text-xs font-semibold hover:underline ${
            current ? 'text-accent' : ''
          }`}
        >
          {track.title}
        </button>
        <span className="block truncate text-[11px] text-muted">{track.user?.username}</span>
      </div>
      <span className="text-[11px] text-muted tabular-nums">
        {formatTime(track.durationMs / 1000)}
      </span>
    </div>
  );
}

function SetRow({ playlist }: { playlist: Playlist }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  return (
    <button
      type="button"
      onClick={() => go({ name: 'playlist', id: playlist.id, secretToken: playlist.secretToken })}
      className="flex w-full items-center gap-2.5 rounded-md py-1 pr-1 text-left hover:bg-hover"
    >
      <Cover
        url={playlist.artworkUrl ?? playlist.user?.avatarUrl}
        size="t67x67"
        className="h-10 w-10 flex-none rounded-md"
      />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-xs font-semibold">{playlist.title}</span>
        <span className="block truncate text-[11px] text-muted">
          {playlist.user && t('trackPage.by', { name: playlist.user.username })}
          {playlist.user && ' · '}
          {t('collection.tracks', { count: playlist.trackCount })}
        </span>
      </span>
    </button>
  );
}

function Sets({ id, albums }: { id: number; albums: boolean }) {
  const { t } = useTranslation();
  const query = useTrackSets(id, albums);
  const [all, setAll] = useState(false);
  const sets = flatten(query.data);
  if (query.isSuccess && sets.length === 0) return null;
  const more = sets.length > SETS_SHOWN || query.hasNextPage;
  const shown = all ? sets : sets.slice(0, SETS_SHOWN);

  return (
    <Block
      title={albums ? t('trackPage.inAlbums') : t('trackPage.inPlaylists')}
      onAll={more ? () => setAll((value) => !value) : undefined}
      allLabel={all ? t('trackPage.collapse') : t('common.all')}
    >
      {query.isLoading && <Skeleton rows={SETS_SHOWN} />}
      {query.isError && <Failed onRetry={() => void query.refetch()} />}
      {shown.map((playlist) => (
        <SetRow key={playlist.id} playlist={playlist} />
      ))}
      {all && query.hasNextPage && (
        <button
          type="button"
          disabled={query.isFetchingNextPage}
          onClick={() => void query.fetchNextPage()}
          className="mt-1 text-[11px] text-muted hover:text-text disabled:opacity-50"
        >
          {t('common.more')}
        </button>
      )}
    </Block>
  );
}

function Likers({ track }: { track: Track }) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const query = useLikers(track.id);
  const users = query.data ?? [];
  if (query.isSuccess && users.length === 0) return null;
  return (
    <Block
      title={t('trackPage.likedBy')}
      extra={track.likesCount !== null ? compact(track.likesCount, i18n.language) : undefined}
    >
      {query.isLoading && <Skeleton rows={1} round />}
      {query.isError && <Failed onRetry={() => void query.refetch()} />}
      <div className="flex -space-x-2">
        {users.slice(0, LIKER_AVATARS).map((user) => (
          <button
            key={user.id}
            type="button"
            title={user.username}
            onClick={() => go({ name: 'artist', id: user.id })}
            className="rounded-full ring-2 ring-bg hover:z-10"
          >
            <Cover url={user.avatarUrl} size="t67x67" className="h-8 w-8 rounded-full" />
          </button>
        ))}
      </div>
    </Block>
  );
}

export function TrackAside({ track, player }: { track: Track; player: PlayerSnapshot | null }) {
  return (
    <aside className="flex flex-col gap-6">
      <Related id={track.id} player={player} />
      <Sets id={track.id} albums={false} />
      <Sets id={track.id} albums />
      <Likers track={track} />
    </aside>
  );
}
