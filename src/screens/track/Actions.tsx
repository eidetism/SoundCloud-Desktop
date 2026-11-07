import {
  Check,
  Ellipsis,
  ExternalLink,
  Heart,
  ListEnd,
  ListPlus,
  ListStart,
  Lock,
  Repeat2,
  Share,
} from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { flatten, useLikedIds, useSetLike } from '../../api/queries';
import {
  useOwnPlaylists,
  usePlaylistSetTrack,
  useRepostedIds,
  useSetRepost,
} from '../../api/track';
import type { Track } from '../../api/types';
import { Cover } from '../../components/Cover';
import { MenuItem, Popover } from '../../components/Popover';
import { appendToQueue, playNext } from '../../player/api';
import { compact, MoreButton, QueryState } from '../common';
import { openLink } from './text';

const COPIED_FOR_MS = 1500;

function Pill({
  active = false,
  title,
  disabled,
  onClick,
  children,
}: {
  active?: boolean;
  title: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={`flex h-8 items-center gap-1.5 rounded-full border px-3 text-xs font-medium disabled:opacity-50 ${
        active ? 'border-accent text-accent' : 'border-line text-text hover:bg-hover'
      }`}
    >
      {children}
    </button>
  );
}

/** "Add to playlist": the user's own playlists, a check on those with the track. */
function PlaylistPicker({ track, onClose }: { track: Track; onClose: () => void }) {
  const { t } = useTranslation();
  const query = useOwnPlaylists(true);
  const edit = usePlaylistSetTrack();
  const playlists = flatten(query.data);
  return (
    <Popover onClose={onClose} className="top-full left-0 mt-1 max-h-80 w-72 overflow-y-auto">
      <p className="px-3.5 pt-1.5 pb-1 text-[11px] text-muted">{t('trackPage.addToPlaylist')}</p>
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && playlists.length === 0 && (
        <p className="px-3.5 py-2 text-xs text-muted">{t('trackPage.noPlaylists')}</p>
      )}
      {playlists.map((playlist) => {
        const has = playlist.trackIds.includes(track.id);
        const busy = edit.isPending && edit.variables?.playlistId === playlist.id;
        return (
          <button
            key={playlist.id}
            type="button"
            disabled={edit.isPending}
            onClick={() =>
              edit.mutate({ playlistId: playlist.id, trackId: track.id, present: !has })
            }
            className="flex w-full items-center gap-2.5 px-3.5 py-1.5 text-left text-xs hover:bg-hover disabled:cursor-wait"
          >
            <Cover url={playlist.artworkUrl} size="t67x67" className="h-8 w-8 flex-none rounded" />
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{playlist.title}</span>
              <span className="flex items-center gap-1 text-[11px] text-muted">
                {playlist.secretToken && <Lock size={10} />}
                {t('collection.tracks', { count: playlist.trackCount })}
              </span>
            </span>
            {busy ? (
              <span className="text-[11px] text-muted">…</span>
            ) : (
              has && <Check size={14} className="text-accent" />
            )}
          </button>
        );
      })}
      <MoreButton
        visible={query.hasNextPage}
        busy={query.isFetchingNextPage}
        onClick={() => void query.fetchNextPage()}
      />
      {edit.isError && (
        <p className="px-3.5 py-1.5 text-[11px] text-danger">{t('trackPage.writeFailed')}</p>
      )}
    </Popover>
  );
}

/** Like, repost, share, add to playlist, more. */
export function TrackActions({ track }: { track: Track }) {
  const { t, i18n } = useTranslation();
  const liked = useLikedIds().data?.has(track.id) ?? false;
  const reposted = useRepostedIds().data?.has(track.id) ?? false;
  const setLike = useSetLike();
  const setRepost = useSetRepost();
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState<'playlists' | 'more' | null>(null);
  const blocked = track.access === 'blocked';

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_FOR_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  // Counters follow the user's own likes/reposts made on this page.
  const [likeDelta, setLikeDelta] = useState(0);
  const [repostDelta, setRepostDelta] = useState(0);
  const shift = (base: number | null, delta: number) =>
    base === null ? null : Math.max(0, base + delta);
  const likes = shift(track.likesCount, likeDelta);
  const reposts = shift(track.repostsCount, repostDelta);
  const close = () => setOpen(null);

  const toggleLike = () => {
    const change = liked ? -1 : 1;
    setLikeDelta((delta) => delta + change);
    setLike.mutate(
      { id: track.id, liked: !liked },
      { onError: () => setLikeDelta((delta) => delta - change) },
    );
  };
  const toggleRepost = () => {
    const change = reposted ? -1 : 1;
    setRepostDelta((delta) => delta + change);
    setRepost.mutate(
      { track, reposted: !reposted },
      { onError: () => setRepostDelta((delta) => delta - change) },
    );
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Pill
        active={liked}
        title={liked ? t('player.unlike') : t('player.like')}
        onClick={toggleLike}
      >
        <Heart size={14} fill={liked ? 'currentColor' : 'none'} />
        {likes !== null && compact(likes, i18n.language)}
      </Pill>
      <Pill
        active={reposted}
        title={reposted ? t('trackPage.unrepost') : t('trackPage.repost')}
        onClick={toggleRepost}
      >
        <Repeat2 size={14} />
        {reposts !== null && compact(reposts, i18n.language)}
      </Pill>
      <Pill
        title={t('trackPage.copyLink')}
        onClick={() =>
          void navigator.clipboard.writeText(track.permalinkUrl).then(() => setCopied(true))
        }
      >
        {copied ? <Check size={14} /> : <Share size={14} />}
        {copied && t('trackPage.copied')}
      </Pill>
      <div className="relative">
        <Pill
          title={t('trackPage.addToPlaylist')}
          onClick={() => setOpen((value) => (value === 'playlists' ? null : 'playlists'))}
        >
          <ListPlus size={14} />
        </Pill>
        {open === 'playlists' && <PlaylistPicker track={track} onClose={close} />}
      </div>
      <div className="relative">
        <Pill
          title={t('track.more')}
          onClick={() => setOpen((value) => (value === 'more' ? null : 'more'))}
        >
          <Ellipsis size={14} />
        </Pill>
        {open === 'more' && (
          <Popover onClose={close} className="top-full left-0 mt-1 w-56">
            {!blocked && (
              <MenuItem
                onClick={() => {
                  void playNext([track.id]);
                  close();
                }}
              >
                <ListStart size={14} /> {t('track.playNext')}
              </MenuItem>
            )}
            {!blocked && (
              <MenuItem
                onClick={() => {
                  void appendToQueue([track.id]);
                  close();
                }}
              >
                <ListEnd size={14} /> {t('track.addToQueue')}
              </MenuItem>
            )}
            <MenuItem
              onClick={() => {
                openLink(track.permalinkUrl);
                close();
              }}
            >
              <ExternalLink size={14} /> {t('trackPage.openInBrowser')}
            </MenuItem>
          </Popover>
        )}
      </div>
      {(setLike.isError || setRepost.isError) && (
        <span className="text-[11px] text-danger">{t('trackPage.writeFailed')}</span>
      )}
    </div>
  );
}
