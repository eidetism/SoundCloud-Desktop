import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { SelectionItem } from '../api/types';
import { playTracks } from '../player/api';
import { type Route, useRouter } from '../shell/router';
import { Cover } from './Cover';
import { PlayCircle } from './PlayCircle';

/** A square artwork tile with a title, optional play button on hover. */
export function Card({
  artwork,
  title,
  subtitle,
  round,
  onOpen,
  onPlay,
}: {
  artwork: string | null | undefined;
  title: string;
  subtitle?: ReactNode;
  round?: boolean;
  onOpen: () => void;
  onPlay?: () => void;
}) {
  return (
    <div className="group w-[150px] flex-none">
      <div className="relative">
        <button type="button" onClick={onOpen} className="block w-full">
          <Cover
            url={artwork}
            size="t300x300"
            className={`aspect-square w-full ${round ? 'rounded-full' : 'rounded-lg'}`}
          />
        </button>
        {onPlay && (
          <div className="absolute right-2 bottom-2 opacity-0 transition-opacity group-hover:opacity-100">
            <PlayCircle playing={false} size={34} onClick={onPlay} />
          </div>
        )}
      </div>
      <button
        type="button"
        title={title}
        onClick={onOpen}
        className={`mt-2 block w-full truncate text-[13px] font-semibold hover:underline ${
          round ? 'text-center' : 'text-left'
        }`}
      >
        {title}
      </button>
      {subtitle && (
        <div className={`truncate text-[11px] text-muted ${round ? 'text-center' : ''}`}>
          {subtitle}
        </div>
      )}
    </div>
  );
}

/** Where a selection item opens. */
export function routeFor(item: SelectionItem): Route {
  switch (item.kind) {
    case 'system-playlist':
      return { name: 'mix', mix: item };
    case 'playlist':
      return { name: 'playlist', id: item.id, secretToken: item.secretToken ?? null };
    case 'user':
      return { name: 'artist', id: item.id };
  }
}

/** A card for a `/mixed-selections` or search item. */
export function SelectionCard({ item }: { item: SelectionItem }) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const open = () => go(routeFor(item));
  switch (item.kind) {
    case 'system-playlist':
      return (
        <Card
          artwork={item.calculatedArtworkUrl}
          title={item.title}
          subtitle={item.shortDescription ?? undefined}
          onOpen={open}
          onPlay={item.trackIds.length > 0 ? () => void playTracks(item.trackIds, 0) : undefined}
        />
      );
    case 'playlist':
      return (
        <Card
          artwork={item.artworkUrl}
          title={item.title}
          subtitle={item.user?.username}
          onOpen={open}
          onPlay={item.trackIds.length > 0 ? () => void playTracks(item.trackIds, 0) : undefined}
        />
      );
    case 'user':
      return (
        <Card
          round
          artwork={item.avatarUrl}
          title={item.username}
          subtitle={
            item.followersCount === null
              ? undefined
              : t('artist.followers', {
                  count: item.followersCount,
                  formatted: new Intl.NumberFormat(i18n.language, { notation: 'compact' }).format(
                    item.followersCount,
                  ),
                })
          }
          onOpen={open}
        />
      );
  }
}

/** A horizontally scrolling row of cards. */
export function Shelf({ children }: { children: ReactNode }) {
  return <div className="-mx-1 flex gap-4 overflow-x-auto px-1 pb-2">{children}</div>;
}
