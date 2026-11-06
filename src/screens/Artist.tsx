import { BadgeCheck, Play } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { flatten, nextPage, useArtistShortcuts, useUser, useUserTracks } from '../api/queries';
import { Card } from '../components/Card';
import { Cover } from '../components/Cover';
import { SectionHeader } from '../components/SectionHeader';
import { TrackList } from '../components/TrackList';
import { type PlayerSnapshot, playTracks } from '../player/api';
import { useRouter } from '../shell/router';
import { compact, QueryState, ScreenTitle } from './common';

/** An artist's profile and uploads. */
export function ArtistScreen({ id, player }: { id: number; player: PlayerSnapshot | null }) {
  const { t, i18n } = useTranslation();
  const user = useUser(id);
  const uploads = useUserTracks(id);
  const tracks = flatten(uploads.data);
  const profile = user.data;
  const stat = (key: string, value: number | null) =>
    value === null ? null : (
      <span>{t(key, { count: value, formatted: compact(value, i18n.language) })}</span>
    );

  return (
    <section>
      <QueryState loading={user.isLoading} error={user.error} onRetry={() => void user.refetch()} />
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
              disabled={tracks.length === 0}
              onClick={() =>
                void playTracks(
                  tracks.map((track) => track.id),
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
      <SectionHeader title={t('artist.uploads')} />
      <QueryState
        loading={uploads.isLoading}
        error={uploads.error}
        onRetry={() => void uploads.refetch()}
      />
      {uploads.isSuccess && tracks.length === 0 && (
        <p className="py-4 text-sm text-muted">{t('artist.noTracks')}</p>
      )}
      <TrackList tracks={tracks} player={player} onEnd={nextPage(uploads)} />
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
