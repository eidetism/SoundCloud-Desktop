import { useTranslation } from 'react-i18next';
import { useTracks } from '../../api/queries';
import { useComments } from '../../api/track';
import type { Me } from '../../auth/api';
import type { PlayerSnapshot } from '../../player/api';
import { QueryState } from '../common';
import { About } from './About';
import { TrackActions } from './Actions';
import { TrackAside } from './Aside';
import { Comments } from './Comments';
import { TrackHeader } from './Header';

/**
 * A track's page, variant A of `docs/design/track-variants.png`. Every block
 * loads on its own: empty ones hide, a failed one doesn't break the page.
 */
export function TrackScreen({
  id,
  player,
  me,
}: {
  id: number;
  player: PlayerSnapshot | null;
  me: Me | null;
}) {
  const { t } = useTranslation();
  const { data, isLoading, error, refetch } = useTracks([id]);
  const track = data?.[0];
  // Shared with the comment list: the header's markers come from the same pages.
  const comments = useComments(
    track?.urn ?? '',
    track?.user ? `soundcloud:users:${track.user.id}` : '',
    track !== undefined,
  );

  if (!track) {
    return (
      <QueryState
        loading={isLoading}
        error={error ?? (data ? t('trackPage.notFound') : null)}
        onRetry={() => void refetch()}
      />
    );
  }

  const markers = comments.data?.pages.flatMap((page) => page.comments) ?? [];

  return (
    <div className="@container flex flex-col gap-5">
      <TrackHeader track={track} player={player} comments={markers} />
      <TrackActions key={track.id} track={track} />
      <div className="grid grid-cols-1 gap-8 @[1100px]:grid-cols-[minmax(0,1fr)_340px]">
        <div className="flex min-w-0 flex-col gap-6">
          <About track={track} />
          <Comments track={track} me={me} player={player} />
        </div>
        <TrackAside track={track} player={player} />
      </div>
    </div>
  );
}
