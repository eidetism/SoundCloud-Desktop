import { useVirtualizer } from '@tanstack/react-virtual';
import { invoke } from '@tauri-apps/api/core';
import { EllipsisVertical } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Track } from '../api/types';
import { type PlayerSnapshot, playTracks, togglePlay } from '../player/api';
import { formatTime } from '../player/usePlayer';
import { useRouter } from '../shell/router';
import { useScrollRoot } from '../shell/scroll';
import { Cover } from './Cover';
import { PlayCircle } from './PlayCircle';

const ROW_HEIGHT = 58;

function RowMenu({ track, onClose }: { track: Track; onClose: () => void }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (!box.current?.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => event.key === 'Escape' && onClose();
    document.addEventListener('pointerdown', close);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', close);
      document.removeEventListener('keydown', key);
    };
  }, [onClose]);

  const item = (label: string, action: () => void) => (
    <button
      type="button"
      onClick={() => {
        action();
        onClose();
      }}
      className="block w-full px-3.5 py-2 text-left text-xs hover:bg-hover"
    >
      {label}
    </button>
  );

  const blocked = track.access === 'blocked';
  return (
    <div
      ref={box}
      data-menu
      className="absolute top-full right-0 z-10 w-52 overflow-hidden rounded-md border border-line bg-bg py-1 shadow-[0_8px_24px_var(--shade)]"
    >
      {!blocked &&
        item(t('track.playNext'), () => void invoke('player_play_next', { ids: [track.id] }))}
      {!blocked &&
        item(t('track.addToQueue'), () => void invoke('player_append', { ids: [track.id] }))}
      {item(t('track.open'), () => go({ name: 'track', id: track.id }))}
      {track.user && item(t('track.artist'), () => go({ name: 'artist', id: track.user!.id }))}
    </div>
  );
}

function Row({
  track,
  index,
  numbered,
  player,
  onPlay,
}: {
  track: Track;
  index: number;
  numbered: boolean;
  player: PlayerSnapshot | null;
  onPlay: () => void;
}) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const [menu, setMenu] = useState(false);
  const blocked = track.access === 'blocked';
  const current = player?.track?.id === track.id;
  const playing = current && (player?.status === 'playing' || player?.status === 'loading');

  return (
    <div
      onDoubleClick={() => !blocked && onPlay()}
      className={`grid h-full grid-cols-[24px_44px_minmax(0,1fr)_auto_26px_20px] items-center gap-3 rounded-md pr-1 text-[13px] hover:bg-hover ${
        blocked ? 'opacity-45' : ''
      }`}
    >
      <span className="text-right text-[11px] text-muted tabular-nums">
        {numbered ? String(index + 1).padStart(2, '0') : ''}
      </span>
      <Cover
        url={track.artworkUrl ?? track.user?.avatarUrl}
        size="t67x67"
        className="h-11 w-11 rounded-lg"
      />
      <div className="min-w-0">
        <button
          type="button"
          title={track.title}
          onClick={() => go({ name: 'track', id: track.id })}
          className={`block max-w-full truncate text-left font-semibold hover:underline ${
            current ? 'text-accent' : ''
          }`}
        >
          {track.title}
        </button>
        <small className="mt-[3px] block truncate text-[11px] text-muted">
          {track.user ? (
            <button
              type="button"
              onClick={() => go({ name: 'artist', id: track.user!.id })}
              className="hover:text-text hover:underline"
            >
              {track.user.username}
            </button>
          ) : null}
          {track.genre ? ` · ${track.genre}` : ''}
        </small>
      </div>
      <span className="text-xs text-muted tabular-nums">
        {blocked
          ? t('library.unavailable')
          : track.access === 'preview'
            ? t('player.preview')
            : formatTime(track.durationMs / 1000)}
      </span>
      <PlayCircle
        playing={playing}
        disabled={blocked}
        onClick={() => (current ? void togglePlay() : onPlay())}
      />
      <div className="relative">
        <button
          type="button"
          title={t('track.more')}
          onClick={() => setMenu((open) => !open)}
          className="grid h-6 w-5 place-items-center text-muted hover:text-text"
        >
          <EllipsisVertical size={15} />
        </button>
        {menu && <RowMenu track={track} onClose={() => setMenu(false)} />}
      </div>
    </div>
  );
}

/**
 * Track rows in the mockup's style, virtualized against the content panel's
 * scroll. Playing a row queues the whole list from it. `onEnd` asks for the
 * next page when the last rows come into view.
 */
export function TrackList({
  tracks,
  player,
  numbered = true,
  onEnd,
}: {
  tracks: Track[];
  player: PlayerSnapshot | null;
  numbered?: boolean;
  onEnd?: () => void;
}) {
  const root = useScrollRoot();
  const list = useRef<HTMLDivElement>(null);
  const [margin, setMargin] = useState(0);

  // The list's offset inside the scrolled content; content above it may grow.
  useLayoutEffect(() => {
    const scroller = root.current;
    const element = list.current;
    if (!scroller || !element) return;
    const measure = () => {
      const top =
        element.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop;
      setMargin((previous) => (Math.abs(previous - top) < 1 ? previous : top));
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (scroller.firstElementChild) observer.observe(scroller.firstElementChild);
    return () => observer.disconnect();
  }, [root]);

  const rows = useVirtualizer({
    count: tracks.length,
    getScrollElement: () => root.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 6,
    scrollMargin: margin,
  });
  const items = rows.getVirtualItems();
  const last = items[items.length - 1]?.index ?? -1;

  useEffect(() => {
    if (onEnd && tracks.length > 0 && last >= tracks.length - 10) onEnd();
  }, [last, tracks.length, onEnd]);

  const ids = tracks.map((track) => track.id);

  return (
    <div ref={list} className="relative" style={{ height: rows.getTotalSize() }}>
      {items.map((row) => (
        <div
          key={row.key}
          className="absolute left-0 w-full has-[[data-menu]]:z-10"
          style={{ height: ROW_HEIGHT, transform: `translateY(${row.start - margin}px)` }}
        >
          <Row
            track={tracks[row.index]}
            index={row.index}
            numbered={numbered}
            player={player}
            onPlay={() => void playTracks(ids, row.index)}
          />
        </div>
      ))}
    </div>
  );
}
