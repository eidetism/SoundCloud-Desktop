import { useTranslation } from 'react-i18next';
import { flatten, nextPage, useDiscover, useFeed, useHistory, useLikes } from '../api/queries';
import { SelectionCard, Shelf } from '../components/Card';
import { SectionHeader } from '../components/SectionHeader';
import { TrackList } from '../components/TrackList';
import type { PlayerSnapshot } from '../player/api';
import { MoreButton, QueryState, ScreenTitle } from './common';
import { feedTracks } from './Home';

type Props = { player: PlayerSnapshot | null };

export function LikesScreen({ player }: Props) {
  const { t } = useTranslation();
  const query = useLikes();
  const tracks = flatten(query.data).map((like) => like.track);
  return (
    <section>
      <ScreenTitle>{t('nav.likes')}</ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      <TrackList tracks={tracks} player={player} onEnd={nextPage(query)} />
    </section>
  );
}

export function HistoryScreen({ player }: Props) {
  const { t } = useTranslation();
  const query = useHistory();
  const tracks = flatten(query.data).map((item) => item.track);
  return (
    <section>
      <ScreenTitle>{t('nav.history')}</ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      <TrackList tracks={tracks} player={player} onEnd={nextPage(query)} />
    </section>
  );
}

export function FeedScreen({ player }: Props) {
  const { t } = useTranslation();
  const query = useFeed();
  return (
    <section>
      <ScreenTitle>{t('nav.feed')}</ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      <TrackList
        tracks={feedTracks(flatten(query.data))}
        player={player}
        numbered={false}
        onEnd={nextPage(query)}
      />
    </section>
  );
}

/** Every `/mixed-selections` shelf: mixes, stations, curated playlists, artists. */
export function DiscoverScreen() {
  const { t } = useTranslation();
  const query = useDiscover();
  const selections = flatten(query.data).filter(
    (selection) => selection.items.collection.length > 0,
  );
  return (
    <section>
      <ScreenTitle>{t('nav.discover')}</ScreenTitle>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {selections.map((selection) => (
        <div key={selection.urn}>
          <SectionHeader title={selection.title ?? ''} />
          <Shelf>
            {selection.items.collection.map((item, i) => (
              <SelectionCard key={`${i}-${item.kind}`} item={item} />
            ))}
          </Shelf>
        </div>
      ))}
      <MoreButton
        visible={query.hasNextPage}
        busy={query.isFetchingNextPage}
        onClick={() => void query.fetchNextPage()}
      />
    </section>
  );
}
