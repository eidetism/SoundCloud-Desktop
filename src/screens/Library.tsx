import { Lock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { flatten, useLibrary } from '../api/queries';
import type { LibraryItem } from '../api/types';
import { Card } from '../components/Card';
import { playTracks } from '../player/api';
import { useRouter } from '../shell/router';
import { QueryState, ScreenTitle } from './common';

type Filter = 'all' | 'created' | 'liked' | 'albums' | 'mixes';
const FILTERS: Filter[] = ['all', 'created', 'liked', 'albums', 'mixes'];

function matches(item: LibraryItem, filter: Filter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'created':
      return item.type === 'playlist';
    case 'liked':
      return item.type === 'playlist-like' && !item.playlist.isAlbum;
    case 'albums':
      return item.type !== 'system-playlist-like' && item.playlist.isAlbum;
    case 'mixes':
      return item.type === 'system-playlist-like';
  }
}

function LibraryCard({ item }: { item: LibraryItem }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  if (item.type === 'system-playlist-like') {
    const mix = item.systemPlaylist;
    return (
      <Card
        artwork={mix.calculatedArtworkUrl}
        title={mix.title}
        subtitle={t('collection.mix')}
        onOpen={() => go({ name: 'mix', mix })}
        onPlay={mix.trackIds.length > 0 ? () => void playTracks(mix.trackIds, 0) : undefined}
      />
    );
  }
  const { playlist } = item;
  const kind = playlist.isAlbum ? t('collection.album') : t('collection.playlist');
  return (
    <Card
      artwork={playlist.artworkUrl ?? playlist.user?.avatarUrl}
      title={playlist.title}
      subtitle={
        <span className="inline-flex items-center gap-1">
          {playlist.secretToken && <Lock size={10} aria-label={t('library.private')} />}
          {item.type === 'playlist'
            ? t('collection.tracks', { count: playlist.trackCount })
            : `${kind} · ${playlist.user?.username ?? ''}`}
        </span>
      }
      onOpen={() => go({ name: 'playlist', id: playlist.id, secretToken: playlist.secretToken })}
    />
  );
}

/**
 * «My playlists»: `/me/library/all` — created, liked playlists and albums,
 * liked mixes. With `albumsOnly` it's the sidebar's Albums screen: the same
 * library locked to albums, no filter chips (the web uses the same endpoint).
 */
export function LibraryScreen({ albumsOnly = false }: { albumsOnly?: boolean }) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<Filter>(albumsOnly ? 'albums' : 'all');
  const query = useLibrary();
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = query;

  // A library is a few dozen entries: load it whole so filters see everything.
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const items = flatten(query.data).filter((item) => matches(item, filter));

  return (
    <section>
      <ScreenTitle>{albumsOnly ? t('nav.albums') : t('nav.playlists')}</ScreenTitle>
      {!albumsOnly && (
        <div className="mb-4 flex flex-wrap gap-2">
          {FILTERS.map((name) => (
            <button
              key={name}
              type="button"
              onClick={() => setFilter(name)}
              className={`rounded-full px-3.5 py-1.5 text-xs font-semibold ${
                filter === name ? 'bg-text text-bg' : 'bg-card text-muted hover:text-text'
              }`}
            >
              {t(`library.${name}`)}
            </button>
          ))}
        </div>
      )}
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && !hasNextPage && items.length === 0 && (
        <p className="py-6 text-sm text-muted">{t('library.empty')}</p>
      )}
      <div className="flex flex-wrap gap-5">
        {items.map((item) => (
          <LibraryCard
            key={
              item.type === 'system-playlist-like'
                ? item.systemPlaylist.urn
                : `${item.type}-${item.playlist.id}`
            }
            item={item}
          />
        ))}
      </div>
    </section>
  );
}
