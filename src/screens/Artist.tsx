import { BadgeCheck, Play } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  flatten,
  nextPage,
  useArtistShortcuts,
  useUser,
  useUserPlaylists,
  useUserTopTracks,
  useUserTracks,
} from '../api/queries';
import { Card, SelectionCard } from '../components/Card';
import { Cover } from '../components/Cover';
import { TrackList } from '../components/TrackList';
import { type PlayerSnapshot, playTracks } from '../player/api';
import { useRouter } from '../shell/router';
import { compact, MoreButton, QueryState, ScreenTitle } from './common';

type Tab = 'popular' | 'tracks' | 'albums' | 'playlists';
const TABS: [Tab, string][] = [
  ['popular', 'artist.popular'],
  ['tracks', 'artist.uploads'],
  ['albums', 'artist.albumsTab'],
  ['playlists', 'artist.playlistsTab'],
];

/** Track tabs: most played (`toptracks`) or uploads, newest first. */
function ArtistTracks({
  id,
  top,
  player,
}: {
  id: number;
  top: boolean;
  player: PlayerSnapshot | null;
}) {
  const { t } = useTranslation();
  const popular = useUserTopTracks(id, top);
  const uploads = useUserTracks(id, !top);
  const query = top ? popular : uploads;
  const tracks = flatten(query.data);
  return (
    <>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && tracks.length === 0 && (
        <p className="py-4 text-sm text-muted">{t('artist.noTracks')}</p>
      )}
      <TrackList tracks={tracks} player={player} onEnd={nextPage(query)} />
    </>
  );
}

/** Albums or other playlists of the artist, as cards. */
function ArtistPlaylists({ id, albums }: { id: number; albums: boolean }) {
  const { t } = useTranslation();
  const query = useUserPlaylists(id, albums);
  const playlists = flatten(query.data);
  return (
    <>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && playlists.length === 0 && (
        <p className="py-4 text-sm text-muted">
          {albums ? t('artist.noAlbums') : t('artist.noPlaylists')}
        </p>
      )}
      <div className="flex flex-wrap gap-5">
        {playlists.map((playlist) => (
          <SelectionCard key={playlist.id} item={{ ...playlist, kind: 'playlist' }} />
        ))}
      </div>
      <MoreButton
        visible={query.hasNextPage}
        busy={query.isFetchingNextPage}
        onClick={() => void query.fetchNextPage()}
      />
    </>
  );
}

/** An artist's profile: banner, header, then popular tracks, uploads, albums, playlists. */
export function ArtistScreen({ id, player }: { id: number; player: PlayerSnapshot | null }) {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<Tab>('popular');
  const user = useUser(id);
  const popular = useUserTopTracks(id);
  const playable = flatten(popular.data);
  const profile = user.data;
  const stat = (key: string, value: number | null) =>
    value === null ? null : (
      <span>{t(key, { count: value, formatted: compact(value, i18n.language) })}</span>
    );

  return (
    <section>
      <QueryState loading={user.isLoading} error={user.error} onRetry={() => void user.refetch()} />
      {profile?.bannerUrl && (
        <Cover
          url={profile.bannerUrl}
          size="original"
          className="mb-5 aspect-[5/1] max-h-[220px] w-full rounded-lg"
        />
      )}
      {profile && (
        <div className="mb-4 flex items-end gap-6">
          <Cover
            url={profile.avatarUrl}
            size="t500x500"
            className="h-40 w-40 flex-none rounded-full"
          />
          <div className="flex min-w-0 flex-col gap-2 pb-1">
            <h1 className="flex items-center gap-2 text-[28px] leading-tight font-bold">
              <span className="truncate">{profile.username}</span>
              {profile.verified && (
                <BadgeCheck
                  size={22}
                  className="flex-none text-muted"
                  aria-label={t('artist.verified')}
                />
              )}
            </h1>
            {(profile.fullName || profile.city) && (
              <div className="text-sm text-muted">
                {[profile.fullName, profile.city].filter(Boolean).join(' · ')}
              </div>
            )}
            <div className="flex flex-wrap gap-4 text-xs text-muted">
              {stat('artist.followers', profile.followersCount)}
              {stat('artist.tracks', profile.trackCount)}
            </div>
            <button
              type="button"
              disabled={playable.length === 0}
              onClick={() =>
                void playTracks(
                  playable.map((track) => track.id),
                  0,
                )
              }
              className="mt-2 flex w-fit items-center gap-2 rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-on-accent disabled:opacity-50"
            >
              <Play size={16} fill="currentColor" />
              {t('player.play')}
            </button>
          </div>
        </div>
      )}
      {profile?.description && (
        <p className="mb-2 max-w-[720px] text-sm whitespace-pre-line text-muted select-text">
          {profile.description}
        </p>
      )}
      <div className="mt-5 mb-3 flex flex-wrap gap-2">
        {TABS.map(([name, label]) => (
          <button
            key={name}
            type="button"
            onClick={() => setTab(name)}
            className={`rounded-full px-3.5 py-1.5 text-xs font-semibold ${
              tab === name ? 'bg-text text-bg' : 'bg-card text-muted hover:text-text'
            }`}
          >
            {t(label)}
          </button>
        ))}
      </div>
      {(tab === 'popular' || tab === 'tracks') && (
        <ArtistTracks key={tab} id={id} top={tab === 'popular'} player={player} />
      )}
      {tab === 'albums' && <ArtistPlaylists key="albums" id={id} albums />}
      {tab === 'playlists' && <ArtistPlaylists key="playlists" id={id} albums={false} />}
    </section>
  );
}

/** Followed artists, most recently active first. */
export function ArtistsScreen() {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const query = useArtistShortcuts();
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = query;

  // The list is short (one card per followed artist): load it whole.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  return (
    <section>
      <ScreenTitle>{t('nav.artists')}</ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      <div className="flex flex-wrap gap-5">
        {flatten(query.data).map(({ user }) => (
          <Card
            key={user.id}
            round
            artwork={user.avatarUrl}
            title={user.username}
            subtitle={
              user.followersCount === null
                ? undefined
                : t('artist.followers', {
                    count: user.followersCount,
                    formatted: compact(user.followersCount, i18n.language),
                  })
            }
            onOpen={() => go({ name: 'artist', id: user.id })}
          />
        ))}
      </div>
    </section>
  );
}
