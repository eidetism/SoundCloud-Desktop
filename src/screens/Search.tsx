import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { flatten, nextPage, useSearch } from '../api/queries';
import type { SearchItem, SelectionItem, Track } from '../api/types';
import { SelectionCard } from '../components/Card';
import { SectionHeader } from '../components/SectionHeader';
import { TrackList } from '../components/TrackList';
import type { PlayerSnapshot } from '../player/api';
import { MoreButton, QueryState, ScreenTitle } from './common';

type Filter = 'all' | 'tracks' | 'playlists' | 'people';
const FILTERS: Filter[] = ['all', 'tracks', 'playlists', 'people'];
const CARDS_IN_ALL = 12;

function Cards({ items }: { items: SelectionItem[] }) {
  return (
    <div className="flex flex-wrap gap-4">
      {items.map((item) => (
        <SelectionCard key={`${item.kind}-${'id' in item ? item.id : item.urn}`} item={item} />
      ))}
    </div>
  );
}

/** `/search` results (all kinds in one paged list), filtered by kind. */
export function SearchScreen({ query, player }: { query: string; player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<Filter>('all');
  const search = useSearch(query);
  const items: SearchItem[] = flatten(search.data);
  const tracks = items.filter((item): item is Track & { kind: 'track' } => item.kind === 'track');
  const playlists = items.filter((item) => item.kind === 'playlist');
  const people = items.filter((item) => item.kind === 'user');
  const more = nextPage(search);
  const total = search.data?.pages[0]?.totalResults;

  return (
    <section>
      <ScreenTitle
        extra={
          total === undefined ? null : (
            <span className="text-xs text-muted">{t('search.results', { count: total })}</span>
          )
        }
      >
        «{query}»
      </ScreenTitle>
      <div className="mb-2 flex gap-2">
        {FILTERS.map((name) => (
          <button
            key={name}
            type="button"
            onClick={() => setFilter(name)}
            className={`rounded-full px-3.5 py-1.5 text-xs font-semibold ${
              filter === name ? 'bg-text text-bg' : 'bg-card text-muted hover:text-text'
            }`}
          >
            {t(`search.${name}`)}
          </button>
        ))}
      </div>
      <QueryState
        loading={search.isLoading}
        error={search.error}
        onRetry={() => void search.refetch()}
      />
      {search.isSuccess && items.length === 0 && (
        <p className="py-6 text-sm text-muted">{t('search.nothing')}</p>
      )}

      {filter === 'all' && (
        <>
          {people.length > 0 && (
            <>
              <SectionHeader title={t('search.people')} onAll={() => setFilter('people')} />
              <Cards items={people.slice(0, 6)} />
            </>
          )}
          {playlists.length > 0 && (
            <>
              <SectionHeader title={t('search.playlists')} onAll={() => setFilter('playlists')} />
              <Cards items={playlists.slice(0, CARDS_IN_ALL / 2)} />
            </>
          )}
          {tracks.length > 0 && <SectionHeader title={t('search.tracks')} />}
          <TrackList tracks={tracks} player={player} onEnd={more} />
        </>
      )}
      {filter === 'tracks' && <TrackList tracks={tracks} player={player} onEnd={more} />}
      {filter === 'playlists' && <Cards items={playlists} />}
      {filter === 'people' && <Cards items={people} />}
      {(filter === 'playlists' || filter === 'people') && (
        <MoreButton visible={search.hasNextPage} busy={search.isFetchingNextPage} onClick={more} />
      )}
    </section>
  );
}
