import { Play } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { flatten, useArtistShortcuts, useDiscover, useFeed, useTracks } from '../api/queries';
import type { FeedItem, Selection, SystemPlaylist, Track } from '../api/types';
import { Cover } from '../components/Cover';
import { PlayCircle } from '../components/PlayCircle';
import { SectionHeader } from '../components/SectionHeader';
import { TrackList } from '../components/TrackList';
import { type PlayerSnapshot, playTracks } from '../player/api';
import { useRouter } from '../shell/router';
import { compact, QueryState } from './common';

const FEED_ON_HOME = 8;

/** System playlists of all loaded `/mixed-selections` shelves. */
function systemPlaylists(selections: Selection[]): SystemPlaylist[] {
  return selections.flatMap((selection) =>
    selection.items.collection.filter(
      (item): item is SystemPlaylist => item.kind === 'system-playlist',
    ),
  );
}

/** Tracks of feed posts and reposts, without repeats. */
export function feedTracks(items: FeedItem[]): Track[] {
  const seen = new Set<number>();
  const tracks: Track[] = [];
  for (const item of items) {
    if (item.track && !seen.has(item.track.id)) {
      seen.add(item.track.id);
      tracks.push(item.track);
    }
  }
  return tracks;
}

/** Weekly Wave: the accent hero with a collage of its first covers. */
function Hero({ mix }: { mix: SystemPlaylist }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const { data: covers } = useTracks(mix.trackIds.slice(0, 6));
  return (
    <div className="gradient-accent relative h-[200px] overflow-hidden rounded-lg p-[26px] text-on-accent">
      <small className="text-[11px] opacity-85">SoundCloud · {mix.title}</small>
      <button
        type="button"
        onClick={() => go({ name: 'mix', mix })}
        className="mt-1.5 mb-2 block max-w-[300px] text-left text-[26px] leading-[1.15] font-bold"
      >
        {t('home.weeklyTitle')}
      </button>
      <p className="max-w-[260px] text-[11px] opacity-85">
        {mix.shortDescription ?? t('home.weeklyHint')}
      </p>
      <button
        type="button"
        disabled={mix.trackIds.length === 0}
        onClick={() => void playTracks(mix.trackIds, 0)}
        className="mt-4 inline-flex items-center gap-2 rounded-full bg-on-accent px-4 py-2 text-xs font-semibold text-g2"
      >
        <Play size={12} fill="currentColor" />
        {t('home.listen')}
      </button>
      <div className="absolute top-[22px] right-[22px] grid -rotate-[8deg] grid-cols-[repeat(3,64px)] gap-2">
        {(covers ?? []).slice(0, 6).map((track) => (
          <Cover
            key={track.id}
            url={track.artworkUrl ?? track.user?.avatarUrl}
            size="t200x200"
            className="h-16 w-16 rounded-md shadow-[0_8px_20px_var(--shade)]"
          />
        ))}
      </div>
    </div>
  );
}

function MixTile({ mix }: { mix: SystemPlaylist }) {
  const go = useRouter((state) => state.go);
  return (
    <div className="relative flex h-24 items-end overflow-hidden rounded-lg p-2.5">
      <button
        type="button"
        aria-label={mix.title}
        onClick={() => go({ name: 'mix', mix })}
        className="absolute inset-0"
      >
        <Cover url={mix.calculatedArtworkUrl} size="t300x300" className="h-full w-full" />
      </button>
      <div className="relative flex flex-1 items-center justify-between gap-2 rounded-[10px] bg-shade px-2.5 py-2 text-[11px] font-semibold text-on-accent">
        <div className="min-w-0">
          <div className="truncate">{mix.title}</div>
          {mix.shortDescription && (
            <small className="block truncate text-[9px] font-normal opacity-80">
              {mix.shortDescription}
            </small>
          )}
        </div>
        <PlayCircle
          playing={false}
          solid={false}
          disabled={mix.trackIds.length === 0}
          onClick={() => void playTracks(mix.trackIds, 0)}
        />
      </div>
    </div>
  );
}

/** Daily Drops: the orange promo in the right column. */
function DailyDrops({ mix }: { mix: SystemPlaylist }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  return (
    <div className="gradient-accent relative h-[146px] overflow-hidden rounded-lg p-5 text-on-accent">
      <button
        type="button"
        aria-label={mix.title}
        onClick={() => go({ name: 'mix', mix })}
        className="absolute inset-0"
      />
      <h2 className="pointer-events-none relative max-w-[150px] text-lg leading-tight font-bold">
        {t('home.dailyDrops')}
      </h2>
      <Cover
        url={mix.calculatedArtworkUrl}
        size="t200x200"
        className="pointer-events-none absolute top-4 right-4 h-[70px] w-[70px] rounded-lg"
      />
      <span className="absolute bottom-4 left-5">
        <PlayCircle
          playing={false}
          solid={false}
          disabled={mix.trackIds.length === 0}
          onClick={() => void playTracks(mix.trackIds, 0)}
        />
      </span>
    </div>
  );
}

function Following() {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const query = useArtistShortcuts();
  const shortcuts = flatten(query.data).slice(0, 8);
  return (
    <>
      <SectionHeader title={t('home.following')} onAll={() => go({ name: 'artists' })} />
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {shortcuts.map(({ user, hasRead }) => (
        <button
          key={user.id}
          type="button"
          onClick={() => go({ name: 'artist', id: user.id })}
          className="flex w-full items-center gap-3 rounded-md py-[7px] text-left hover:bg-hover"
        >
          <span className="relative flex-none">
            <Cover url={user.avatarUrl} size="t67x67" className="h-11 w-11 rounded-[10px]" />
            {!hasRead && (
              <span className="absolute -top-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-bg bg-accent" />
            )}
          </span>
          <span className="min-w-0">
            <b className="block truncate text-[13px]">{user.username}</b>
            {user.followersCount !== null && (
              <small className="text-[11px] text-muted">
                {t('artist.followers', {
                  count: user.followersCount,
                  formatted: compact(user.followersCount, i18n.language),
                })}
              </small>
            )}
          </span>
        </button>
      ))}
    </>
  );
}

/** Home from the mockup: Weekly Wave, your mixes and the feed; Daily Drops and following on the right. */
export function HomeScreen({ player }: { player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const discover = useDiscover();
  const feed = useFeed();
  const mixes = systemPlaylists(flatten(discover.data));
  const weekly = mixes.find((mix) => mix.trackingFeatureName === 'weekly');
  const daily = mixes.find((mix) => mix.trackingFeatureName === 'new-for-you');
  const yourMixes = mixes.filter((mix) => mix.trackingFeatureName === 'your-moods');
  const uniqueMixes = [...new Map(yourMixes.map((mix) => [mix.urn, mix])).values()].slice(0, 3);
  const tracks = feedTracks(flatten(feed.data)).slice(0, FEED_ON_HOME);

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_260px] gap-6">
      <section className="min-w-0">
        <QueryState
          loading={discover.isLoading}
          error={discover.error}
          onRetry={() => void discover.refetch()}
        />
        {weekly && <Hero mix={weekly} />}
        {uniqueMixes.length > 0 && (
          <>
            <SectionHeader title={t('home.mixes')} onAll={() => go({ name: 'discover' })} />
            <div className="grid grid-cols-3 gap-3.5">
              {uniqueMixes.map((mix) => (
                <MixTile key={mix.urn} mix={mix} />
              ))}
            </div>
          </>
        )}
        <SectionHeader title={t('nav.feed')} onAll={() => go({ name: 'feed' })} />
        <QueryState
          loading={feed.isLoading}
          error={feed.error}
          onRetry={() => void feed.refetch()}
        />
        <TrackList tracks={tracks} player={player} />
      </section>
      <aside className="min-w-0">
        {daily && <DailyDrops mix={daily} />}
        <Following />
      </aside>
    </div>
  );
}
