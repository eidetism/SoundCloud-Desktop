import { Heart, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCommentLike, useComments, useCreateComment, useDeleteComment } from '../../api/track';
import type { Comment, Track } from '../../api/types';
import type { Me } from '../../auth/api';
import { Cover } from '../../components/Cover';
import { VirtualStack } from '../../components/VirtualStack';
import type { PlayerSnapshot } from '../../player/api';
import { formatTime, livePosition, usePositionSeconds } from '../../player/usePlayer';
import { useRouter } from '../../shell/router';
import { compact, MoreButton, QueryState } from '../common';
import { playAt } from './Header';
import { RichText, timeAgo } from './text';

const urnOf = (me: Me) => `soundcloud:users:${me.id}`;

/** "Comment at 0:57": the playing position if this track is current, else the start. */
function Composer({
  track,
  me,
  current,
  playing,
  replyTo,
  onCancelReply,
}: {
  track: Track;
  me: Me | null;
  current: boolean;
  playing: boolean;
  replyTo: Comment | null;
  onCancelReply: () => void;
}) {
  const { t } = useTranslation();
  const seconds = usePositionSeconds();
  const create = useCreateComment(track.urn);
  const [text, setText] = useState('');
  const at = replyTo ? replyTo.trackTimeMs / 1000 : current ? seconds : 0;

  const submit = () => {
    const body = text.trim();
    if (!body || create.isPending) return;
    const atMs = replyTo
      ? replyTo.trackTimeMs
      : current
        ? Math.floor(livePosition(playing) * 1000)
        : 0;
    create.mutate(
      {
        atMs,
        body: replyTo ? `@${replyTo.user.permalink}: ${body}` : body,
        parent: replyTo,
      },
      {
        onSuccess: () => {
          setText('');
          onCancelReply();
        },
      },
    );
  };

  return (
    <div className="flex flex-col gap-1.5">
      {replyTo && (
        <div className="flex items-center gap-1.5 pl-11 text-[11px] text-muted">
          {t('trackPage.replyingTo', { name: replyTo.user.username })}
          <button
            type="button"
            title={t('trackPage.cancel')}
            onClick={onCancelReply}
            className="hover:text-text"
          >
            <X size={12} />
          </button>
        </div>
      )}
      <div className="flex items-center gap-3">
        <Cover url={me?.avatarUrl} size="t67x67" className="h-8 w-8 flex-none rounded-full" />
        <input
          value={text}
          maxLength={1000}
          disabled={create.isPending}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') submit();
            if (event.key === 'Escape' && replyTo) onCancelReply();
          }}
          placeholder={t('trackPage.commentAt', { time: formatTime(at) })}
          className="h-9 min-w-0 flex-1 rounded-full bg-card px-4 text-xs outline-none placeholder:text-muted focus:ring-1 focus:ring-accent disabled:opacity-60"
        />
      </div>
      {create.isError && (
        <p className="pl-11 text-[11px] text-danger">{t('trackPage.writeFailed')}</p>
      )}
    </div>
  );
}

function CommentItem({
  comment,
  track,
  current,
  meUrn,
  reply = false,
  expanded,
  onToggleReplies,
  onReply,
}: {
  comment: Comment;
  track: Track;
  current: boolean;
  meUrn: string | null;
  reply?: boolean;
  expanded: boolean;
  onToggleReplies: () => void;
  onReply: (parent: Comment) => void;
}) {
  const { t, i18n } = useTranslation();
  const go = useRouter((state) => state.go);
  const like = useCommentLike(track.urn);
  const remove = useDeleteComment(track.urn);
  const [confirming, setConfirming] = useState(false);
  const own = meUrn !== null && comment.user.urn === meUrn;
  const openUser = () => comment.user.id !== null && go({ name: 'artist', id: comment.user.id });
  const hiddenReplies = comment.repliesTotal - comment.replies.length;

  return (
    <div className={`flex gap-3 ${reply ? 'pt-3' : 'py-3'}`}>
      <button type="button" onClick={openUser} className="flex-none self-start">
        <Cover
          url={comment.user.avatarUrl}
          size="t67x67"
          className={`${reply ? 'h-6 w-6' : 'h-8 w-8'} rounded-full`}
        />
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-2 text-[11px]">
          <button type="button" onClick={openUser} className="font-semibold hover:underline">
            {comment.user.username}
          </button>
          <button
            type="button"
            disabled={track.access === 'blocked'}
            onClick={() => playAt(track, current, comment.trackTimeMs / 1000)}
            className="text-accent tabular-nums hover:underline"
          >
            {t('trackPage.at', { time: formatTime(comment.trackTimeMs / 1000) })}
          </button>
          <span className="text-muted">{timeAgo(comment.createdAt, i18n.language)}</span>
          {comment.likedByCreator && (
            <span className="flex items-center gap-0.5 text-muted">
              <Heart size={10} fill="currentColor" className="text-accent" />
              {t('trackPage.creatorLiked')}
            </span>
          )}
        </div>
        <p className="mt-0.5 text-[13px] whitespace-pre-line select-text [overflow-wrap:anywhere]">
          <RichText text={comment.body} />
        </p>
        <div className="mt-1 flex items-center gap-3 text-[11px] text-muted">
          <button
            type="button"
            title={comment.liked ? t('player.unlike') : t('player.like')}
            onClick={() => like.mutate({ comment, liked: !comment.liked })}
            className={`flex items-center gap-1 ${comment.liked ? 'text-accent' : 'hover:text-text'}`}
          >
            <Heart size={11} fill={comment.liked ? 'currentColor' : 'none'} />
            {comment.likes > 0 && compact(comment.likes, i18n.language)}
          </button>
          <button type="button" onClick={() => onReply(comment)} className="hover:text-text">
            {t('trackPage.reply')}
          </button>
          {own &&
            (confirming ? (
              <span className="flex items-center gap-2">
                {t('trackPage.deleteConfirm')}
                <button
                  type="button"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(comment)}
                  className="text-danger hover:underline"
                >
                  {t('trackPage.yes')}
                </button>
                <button
                  type="button"
                  onClick={() => setConfirming(false)}
                  className="hover:text-text"
                >
                  {t('trackPage.no')}
                </button>
              </span>
            ) : (
              <button type="button" onClick={() => setConfirming(true)} className="hover:text-text">
                {t('trackPage.delete')}
              </button>
            ))}
          {(like.isError || remove.isError) && (
            <span className="text-danger">{t('trackPage.writeFailed')}</span>
          )}
        </div>
        {!reply && comment.replies.length > 0 && (
          <button
            type="button"
            onClick={onToggleReplies}
            className="mt-1.5 text-[11px] font-medium text-accent hover:underline"
          >
            {expanded
              ? t('trackPage.hideReplies')
              : t('trackPage.showReplies', { count: comment.repliesTotal })}
          </button>
        )}
        {!reply && expanded && (
          <div className="border-l border-line pl-3">
            {comment.replies.map((child) => (
              <CommentItem
                key={child.urn}
                comment={child}
                track={track}
                current={current}
                meUrn={meUrn}
                reply
                expanded={false}
                onToggleReplies={() => {}}
                onReply={() => onReply(comment)}
              />
            ))}
            {hiddenReplies > 0 && (
              <p className="pt-2 text-[11px] text-muted">
                {t('trackPage.moreRepliesOnSite', { count: hiddenReplies })}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export function Comments({
  track,
  me,
  player,
}: {
  track: Track;
  me: Me | null;
  player: PlayerSnapshot | null;
}) {
  const { t, i18n } = useTranslation();
  const query = useComments(track.urn, track.user ? `soundcloud:users:${track.user.id}` : '');
  const [replyTo, setReplyTo] = useState<Comment | null>(null);
  const [open, setOpen] = useState<Set<string>>(() => new Set());
  const current = player?.track?.id === track.id;
  const playing = current && player?.status === 'playing';
  const comments = query.data?.pages.flatMap((page) => page.comments) ?? [];
  const meUrn = me ? urnOf(me) : null;

  const toggle = (urn: string) =>
    setOpen((previous) => {
      const next = new Set(previous);
      if (!next.delete(urn)) next.add(urn);
      return next;
    });

  return (
    <section>
      <div className="mb-3 flex items-baseline justify-between">
        <b className="text-[15px]">{t('trackPage.comments')}</b>
        {track.commentCount !== null && (
          <span className="text-[11px] text-muted">
            {compact(track.commentCount, i18n.language)}
          </span>
        )}
      </div>
      <Composer
        track={track}
        me={me}
        current={current}
        playing={playing}
        replyTo={replyTo}
        onCancelReply={() => setReplyTo(null)}
      />
      <QueryState
        loading={query.isLoading}
        error={query.error}
        onRetry={() => void query.refetch()}
      />
      {query.isSuccess && comments.length === 0 && (
        <p className="py-4 text-sm text-muted">{t('trackPage.noComments')}</p>
      )}
      <div className="mt-2">
        <VirtualStack
          items={comments}
          itemKey={(comment) => comment.urn}
          estimate={84}
          render={(comment) => (
            <CommentItem
              comment={comment}
              track={track}
              current={current}
              meUrn={meUrn}
              expanded={open.has(comment.urn)}
              onToggleReplies={() => toggle(comment.urn)}
              onReply={(parent) => {
                setReplyTo(parent);
                setOpen((previous) => new Set(previous).add(parent.urn));
              }}
            />
          )}
        />
      </div>
      <MoreButton
        visible={query.hasNextPage}
        busy={query.isFetchingNextPage}
        onClick={() => void query.fetchNextPage()}
      />
    </section>
  );
}
