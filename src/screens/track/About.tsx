import { BadgeCheck } from 'lucide-react';
import { useLayoutEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useFollowingIds } from '../../api/track';
import type { Track } from '../../api/types';
import { Cover } from '../../components/Cover';
import { useRouter } from '../../shell/router';
import { compact } from '../common';
import { RichText } from './text';

/** Uploader with counters. Following needs an endpoint not captured yet: the button is off. */
function Author({ track }: { track: Track }) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const following = useFollowingIds().data;
  const user = track.user;
  if (!user) return null;
  const followed = following?.has(user.id) ?? false;
  const stats = [
    user.followersCount !== null &&
      t('artist.followers', {
        count: user.followersCount,
        formatted: compact(user.followersCount, i18n.language),
      }),
    user.trackCount !== null &&
      t('artist.tracks', {
        count: user.trackCount,
        formatted: compact(user.trackCount, i18n.language),
      }),
  ].filter(Boolean);

  return (
    <div className="flex items-center gap-3">
      <button type="button" onClick={() => go({ name: 'artist', id: user.id })}>
        <Cover url={user.avatarUrl} size="t67x67" className="h-11 w-11 rounded-full" />
      </button>
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={() => go({ name: 'artist', id: user.id })}
          className="flex max-w-full items-center gap-1 text-sm font-semibold hover:underline"
        >
          <span className="truncate">{user.username}</span>
          {user.verified && (
            <BadgeCheck
              size={14}
              className="flex-none text-accent"
              aria-label={t('artist.verified')}
            />
          )}
        </button>
        {stats.length > 0 && <p className="text-[11px] text-muted">{stats.join(' · ')}</p>}
      </div>
      <button
        type="button"
        disabled
        title={t('trackPage.followSoon')}
        className="rounded-full border border-accent px-3.5 py-1.5 text-xs font-medium text-accent disabled:opacity-60"
      >
        {followed ? t('trackPage.following') : t('trackPage.follow')}
      </button>
    </div>
  );
}

/** Description with links, collapsed after four lines. */
function Description({ text }: { text: string }) {
  const { t } = useTranslation();
  const box = useRef<HTMLParagraphElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);

  useLayoutEffect(() => {
    const element = box.current;
    if (!element || expanded) return;
    const measure = () => setOverflows(element.scrollHeight > element.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [expanded]);

  return (
    <div>
      <p
        ref={box}
        className={`text-[13px] leading-relaxed whitespace-pre-line select-text [overflow-wrap:anywhere] ${
          expanded ? '' : 'line-clamp-4'
        }`}
      >
        <RichText text={text} />
      </p>
      {(overflows || expanded) && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="mt-1 text-xs text-muted hover:text-text"
        >
          {expanded ? t('trackPage.less') : t('trackPage.moreText')}
        </button>
      )}
    </div>
  );
}

export function About({ track }: { track: Track }) {
  const go = useRouter((state) => state.go);
  const description = track.description?.trim();
  return (
    <section className="flex flex-col gap-4">
      <Author track={track} />
      {description && <Description text={description} />}
      {track.tags.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {track.tags.map((tag) => (
            <button
              key={tag}
              type="button"
              onClick={() => go({ name: 'search', query: tag })}
              className="rounded-full bg-card px-3 py-1 text-[11px] text-muted hover:text-text"
            >
              #{tag}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
