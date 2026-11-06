import { Play } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { flatten, useArtistShortcuts, useDiscover, useFeed, useTracks } from '../api/queries';
import type { FeedItem, Selection, SystemPlaylist, Track } from '../api/types';
import { Cover } from '../components/Cover';
import { PlayCircle } from '../components/PlayCircle';
import { SectionHeader } from '../components/SectionHeader';
import { TrackList } from '../components/TrackList';
import { type PlayerSnapshot, playTracks, togglePlay } from '../player/api';
import { formatTime } from '../player/usePlayer';
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

const HERO_PREVIEW = 4;
const DROPS_STACK = 4;

/** One Weekly Wave track in the hero: play from it, open it, pause if current. */
function HeroTrack({
  track,
  index,
  mix,
  player,
}: {
  track: Track;
  index: number;
  mix: SystemPlaylist;
  player: PlayerSnapshot | null;
}) {
  const go = useRouter((state) => state.go);
  const current = player?.track?.id === track.id;
  const playing = current && (player?.status === 'playing' || player?.status === 'loading');
  return (
    <div
      className={`flex items-center gap-3 rounded-[10px] px-2 py-1.5 ${
        index === HERO_PREVIEW - 1 ? '@max-[1100px]:hidden' : ''
      } ${current ? 'bg-hero-shade' : ''}`}
    >
      <Cover
        url={track.artworkUrl ?? track.user?.avatarUrl}
        size="t67x67"
        blank="bg-hero-shade"
        className="h-9 w-9 flex-none rounded-md"
      />
      <div className="min-w-0 flex-1">
        <button
          type="button"
          title={track.title}
          onClick={() => go({ name: 'track', id: track.id })}
          className="block max-w-full truncate text-left text-[13px] font-semibold hover:underline"
        >
          {track.title}
        </button>
        <div className="truncate text-[11px] opacity-80">{track.user?.username}</div>
      </div>
      <span className="flex-none text-xs tabular-nums opacity-85">
        {formatTime(track.durationMs / 1000)}
      </span>
      <PlayCircle
        playing={playing}
        solid={false}
        disabled={track.access === 'blocked'}
        onClick={() => void (current ? togglePlay() : playTracks(mix.trackIds, index))}
      />
    </div>
  );
}

/** Weekly Wave hero (variant A): text and «Listen» on the left, its first tracks on the right. */
function Hero({ mix, player }: { mix: SystemPlaylist; player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const preview = mix.trackIds.slice(0, HERO_PREVIEW);
  const { data: tracks } = useTracks(preview);
  return (
    <div className="gradient-hero grid h-full min-h-[220px] grid-cols-[340px_minmax(0,1fr)] items-center gap-7 rounded-[18px] p-7 text-on-accent @max-[900px]:grid-cols-1">
      <div className="min-w-0">
        <small className="text-[11px] opacity-85">
          {t('home.weeklyCaption', { title: mix.title })}
        </small>
        <button
          type="button"
          onClick={() => go({ name: 'mix', mix })}
          className="mt-1.5 mb-2 line-clamp-2 block text-left text-[28px] leading-[1.15] font-bold"
        >
          {t('home.weeklyTitle')}
        </button>
        <p className="text-xs opacity-85">{t('home.weeklyHint')}</p>
        <button
          type="button"
          disabled={mix.trackIds.length === 0}
          onClick={() => void playTracks(mix.trackIds, 0)}
          className="mt-4 inline-flex items-center gap-2 rounded-full bg-on-accent px-4 py-2 text-xs font-semibold text-accent disabled:opacity-60"
        >
          <Play size={12} fill="currentColor" />
          {t('home.listen')}
        </button>
      </div>
      {preview.length > 0 && (
        <div className="min-w-0 rounded-[14px] bg-hero-shade p-2 @max-[900px]:hidden">
          {tracks
            ? tracks.map((track, index) => (
                <HeroTrack key={track.id} track={track} index={index} mix={mix} player={player} />
              ))
            : preview.map((id, index) => (
                <div
                  key={id}
                  className={`h-12 ${index === HERO_PREVIEW - 1 ? '@max-[1100px]:hidden' : ''}`}
                />
              ))}
        </div>
      )}
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

/** Daily Drops (variant A): a dark card beside the hero, the same height. */
function DailyDrops({ mix }: { mix: SystemPlaylist }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const { data: covers } = useTracks(mix.trackIds.slice(0, DROPS_STACK));
  return (
    <div className="relative flex h-full min-h-[220px] flex-col rounded-[18px] bg-promo-dark p-5 text-side-hi">
      <button
        type="button"
        aria-label={mix.title}
        onClick={() => go({ name: 'mix', mix })}
        className="absolute inset-0 rounded-[18px]"
      />
      <small className="pointer-events-none relative text-[11px] font-semibold text-accent">
        {mix.title}
      </small>
      <h2 className="pointer-events-none relative mt-1 text-lg leading-tight font-bold">
        {t('home.dailyTitle')}
      </h2>
      <div className="mt-auto flex items-end justify-between pt-4">
        <div className="pointer-events-none relative flex">
          {(covers ?? []).map((track, index) => (
            <Cover
              key={track.id}
              url={track.artworkUrl ?? track.user?.avatarUrl}
              size="t67x67"
              blank="bg-side-2"
              className={`h-10 w-10 rounded-lg border-2 border-promo-dark ${index > 0 ? '-ml-2.5' : ''}`}
            />
          ))}
        </div>
        <span className="relative">
          <PlayCircle
            playing={false}
            size={36}
            disabled={mix.trackIds.length === 0}
            onClick={() => void playTracks(mix.trackIds, 0)}
          />
        </span>
      </div>
    </div>
  );
}

/** Daily Drops placeholder; the grid row stretches it to the hero's height. */
function Skeleton({ className }: { className: string }) {
  return <div className={`min-h-[220px] animate-pulse rounded-[18px] bg-card ${className}`} />;
}

/** Hero placeholder with the hero's own text, invisible, so the height matches exactly. */
function HeroSkeleton() {
  const { t } = useTranslation();
  return (
    <div className="grid h-full min-h-[220px] animate-pulse grid-cols-[340px_minmax(0,1fr)] items-center gap-7 rounded-[18px] bg-card p-7 @max-[900px]:grid-cols-1">
      <div className="invisible min-w-0" aria-hidden>
        <small className="text-[11px]">{t('home.weeklyCaption', { title: 'Weekly Wave' })}</small>
        <div className="mt-1.5 mb-2 line-clamp-2 text-[28px] leading-[1.15] font-bold">
          {t('home.weeklyTitle')}
        </div>
        <p className="text-xs">{t('home.weeklyHint')}</p>
        <span className="mt-4 inline-flex items-center gap-2 px-4 py-2 text-xs font-semibold">
          <Play size={12} />
          {t('home.listen')}
        </span>
      </div>
      <div className="invisible p-2 @max-[900px]:hidden" aria-hidden>
        {Array.from({ length: HERO_PREVIEW }, (_, index) => (
          <div
            key={index}
            className={`h-12 ${index === HERO_PREVIEW - 1 ? '@max-[1100px]:hidden' : ''}`}
          />
        ))}
      </div>
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

/**
 * Home: Weekly Wave and Daily Drops share the first grid row (same top and
 * height); your mixes and the feed below on the left, following on the right.
 */
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
  // Section headers add 20px on top: 8 + 20 = 28px under the first row.
  const top = 'row-start-1 mb-2 min-w-0';

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_260px] items-stretch gap-x-6">
      {discover.isLoading ? (
        <>
          <div className={`${top} @container col-start-1`}>
            <HeroSkeleton />
          </div>
          <Skeleton className={`${top} col-start-2`} />
        </>
      ) : (
        <>
          {weekly && (
            <div className={`${top} @container col-start-1 ${daily ? '' : 'col-span-2'}`}>
              <Hero mix={weekly} player={player} />
            </div>
          )}
          {daily && (
            <div className={`${top} ${weekly ? 'col-start-2' : 'col-span-2 col-start-1'}`}>
              <DailyDrops mix={daily} />
            </div>
          )}
        </>
      )}
      <section className="col-start-1 row-start-2 min-w-0">
        <QueryState
          loading={false}
          error={discover.error}
          onRetry={() => void discover.refetch()}
        />
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
      <aside className="col-start-2 row-start-2 min-w-0">
        <Following />
      </aside>
    </div>
  );
}
