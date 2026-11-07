import { Heart, MessageCircle, Pause, Play, Repeat2 } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Comment, Track } from '../../api/types';
import { Cover } from '../../components/Cover';
import { type PlayerSnapshot, playTracks, seek, togglePlay } from '../../player/api';
import { formatTime, notePosition, usePositionSeconds } from '../../player/usePlayer';
import { Waveform } from '../../player/Waveform';
import { useRouter } from '../../shell/router';
import { compact } from '../common';

/** Most avatars shown on the waveform; denser spots are grouped. */
const MAX_MARKERS = 40;
const MARKER_SPACING = 22;

/**
 * Start the track at `seconds`: seek if it is the current one, otherwise
 * play it alone from there.
 */
export function playAt(track: Track, current: boolean, seconds: number) {
  if (current) {
    notePosition(seconds);
    void seek(seconds);
  } else {
    void playTracks([track.id], 0, seconds);
  }
}

function Elapsed({ active }: { active: boolean }) {
  const seconds = usePositionSeconds();
  return <span>{formatTime(active ? seconds : 0)}</span>;
}

interface Marker {
  left: number;
  seconds: number;
  comments: Comment[];
}

/** Commenters' avatars along the waveform, grouped where they crowd. */
function CommentMarkers({
  comments,
  durationMs,
  onPick,
}: {
  comments: Comment[];
  durationMs: number;
  onPick: (seconds: number) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const markers = useMemo(() => {
    if (width === 0 || durationMs <= 0) return [];
    const groups = new Map<number, Marker>();
    for (const comment of comments) {
      const fraction = Math.min(1, Math.max(0, comment.trackTimeMs / durationMs));
      const slot = Math.round((fraction * width) / MARKER_SPACING);
      const group = groups.get(slot);
      if (group) group.comments.push(comment);
      else
        groups.set(slot, {
          left: Math.min(1, (slot * MARKER_SPACING) / width) * 100,
          seconds: comment.trackTimeMs / 1000,
          comments: [comment],
        });
    }
    return [...groups.values()]
      .sort((a, b) => b.comments.length - a.comments.length)
      .slice(0, MAX_MARKERS);
  }, [comments, durationMs, width]);

  return (
    <div ref={box} className="relative h-5">
      {markers.map((marker) => {
        const [first] = marker.comments;
        return (
          <button
            key={marker.left}
            type="button"
            onClick={() => onPick(marker.seconds)}
            className="group absolute top-0 -translate-x-1/2 hover:z-10"
            style={{ left: `${marker.left}%` }}
          >
            <Cover
              url={first.user.avatarUrl}
              size="t67x67"
              blank="bg-side-2"
              className="h-[18px] w-[18px] rounded-full ring-1 ring-head-fade"
            />
            <span className="pointer-events-none absolute top-6 left-1/2 hidden w-max max-w-64 -translate-x-1/2 rounded-md bg-side-2 px-2 py-1 text-left text-[11px] text-side-hi group-hover:block">
              <b>{first.user.username}</b> · {formatTime(first.trackTimeMs / 1000)}
              <span className="mt-0.5 line-clamp-2 block text-side-text">{first.body}</span>
              {marker.comments.length > 1 && (
                <span className="text-side-text">+{marker.comments.length - 1}</span>
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** Variant A's header: stretched artwork behind, play, stats, a big waveform. */
export function TrackHeader({
  track,
  player,
  comments,
}: {
  track: Track;
  player: PlayerSnapshot | null;
  comments: Comment[];
}) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const current = player?.track?.id === track.id;
  const playing = current && player?.status === 'playing';
  const blocked = track.access === 'blocked';
  const count = (value: number | null) => (value === null ? '—' : compact(value, i18n.language));
  const date = track.displayDate
    ? new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium' }).format(
        new Date(track.displayDate),
      )
    : null;

  return (
    <header className="relative overflow-hidden rounded-xl bg-promo-dark text-side-hi">
      {/* A 67 px image stretched over the header reads as blurred, without a filter. */}
      <Cover
        url={track.artworkUrl ?? track.user?.avatarUrl}
        size="t67x67"
        blank="bg-promo-dark"
        className="absolute inset-0 h-full w-full scale-110"
      />
      <div className="absolute inset-0 bg-linear-to-b from-head-shade to-head-fade" />
      <div className="relative flex flex-col gap-5 p-6">
        <div className="flex items-end gap-6">
          <Cover
            url={track.artworkUrl ?? track.user?.avatarUrl}
            size="t500x500"
            blank="bg-side-2"
            className="h-[180px] w-[180px] flex-none rounded-lg shadow-[0_8px_24px_var(--shade)]"
          />
          <div className="flex min-w-0 items-center gap-5 pb-2">
            <button
              type="button"
              disabled={blocked}
              title={playing ? t('player.pause') : t('player.play')}
              onClick={() => void (current ? togglePlay() : playTracks([track.id], 0))}
              className="grid h-[60px] w-[60px] flex-none place-items-center rounded-full bg-accent text-on-accent disabled:opacity-50"
            >
              {playing ? (
                <Pause size={24} fill="currentColor" />
              ) : (
                <Play size={24} fill="currentColor" className="ml-1" />
              )}
            </button>
            <div className="min-w-0">
              {track.user && (
                <button
                  type="button"
                  onClick={() => go({ name: 'artist', id: track.user!.id })}
                  className="text-sm text-side-text hover:text-side-hi hover:underline"
                >
                  {track.user.username}
                </button>
              )}
              <h1 className="text-[28px] leading-tight font-bold [overflow-wrap:anywhere]">
                {track.title}
              </h1>
              <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-side-text">
                {track.genre && <span className="text-accent">{track.genre}</span>}
                <span>{formatTime(track.durationMs / 1000)}</span>
                <span className="flex items-center gap-1">
                  <Play size={12} /> {count(track.playbackCount)}
                </span>
                <span className="flex items-center gap-1">
                  <Heart size={12} /> {count(track.likesCount)}
                </span>
                <span className="flex items-center gap-1">
                  <Repeat2 size={12} /> {count(track.repostsCount)}
                </span>
                <span className="flex items-center gap-1">
                  <MessageCircle size={12} /> {count(track.commentCount)}
                </span>
                {date && <span>{date}</span>}
                {blocked && <span>{t('library.unavailable')}</span>}
                {track.access === 'preview' && <span>{t('player.preview')}</span>}
              </div>
            </div>
          </div>
        </div>
        <div>
          <Waveform
            track={blocked ? null : track}
            playing={playing}
            active={current}
            onSeek={(seconds) => playAt(track, current, seconds)}
            barWidth={3}
            restColor="--wave"
            className="h-20 w-full"
          />
          <CommentMarkers
            comments={comments}
            durationMs={track.durationMs}
            onPick={(seconds) => !blocked && playAt(track, current, seconds)}
          />
          <div className="mt-1 flex justify-between text-[11px] text-side-text tabular-nums">
            <Elapsed active={current} />
            <span>{formatTime(track.durationMs / 1000)}</span>
          </div>
        </div>
      </div>
    </header>
  );
}
