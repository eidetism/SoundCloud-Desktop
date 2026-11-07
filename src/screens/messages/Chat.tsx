import { useVirtualizer } from '@tanstack/react-virtual';
import { Play, SendHorizontal } from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  type Message,
  useConversation,
  useLinkedTrack,
  useMarkRead,
  useMessages,
  usePendingMessage,
  useSendMessage,
} from '../../api/messages';
import { Cover } from '../../components/Cover';
import { playTracks } from '../../player/api';
import { useRouter } from '../../shell/router';
import { RichText } from '../track/text';

const ESTIMATE = 56;
/** Older messages load when the view is this close to the top. */
const LOAD_OLDER_AT = 240;
/** "At the bottom" for following new messages. */
const BOTTOM_SLACK = 80;
const MAX_INPUT_HEIGHT = 120;
const MAX_PREVIEWS = 3;
const TRACK_LINK = /https:\/\/(?:www\.|m\.|on\.)?soundcloud\.com\/[^\s<>"]+/g;
const TRAILING = /[.,;:!?)\]]+$/;

type Entry =
  | { kind: 'day'; key: string; label: string }
  | { kind: 'message'; key: string; message: Message }
  | { kind: 'pending'; key: string; body: string };

/** One conversation: header, messages (oldest at the top) and the composer. */
export function Chat({ withId }: { withId: string }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const conversation = useConversation(withId);
  const markRead = useMarkRead();
  const peer = conversation.data?.peer ?? null;
  const system = withId === 'system';

  // Open = read, as on the site; again when a new message comes in.
  const last = conversation.data?.lastMessage;
  const unread = conversation.data !== undefined && !conversation.data.read && !last?.mine;
  // biome-ignore lint/correctness/useExhaustiveDependencies: once per new message.
  useEffect(() => {
    if (unread) markRead.mutate(withId);
  }, [unread, last?.sentAt, withId]);

  if (conversation.isError) {
    return <p className="m-auto text-[13px] text-muted">{t('messages.notFound')}</p>;
  }

  return (
    <>
      <header className="flex flex-none items-center gap-3 border-b border-line px-5 py-3">
        <Cover url={peer?.avatarUrl} size="t67x67" className="h-9 w-9 flex-none rounded-full" />
        {peer && !system ? (
          <button
            type="button"
            onClick={() => go({ name: 'artist', id: peer.id })}
            className="truncate text-[14px] font-bold hover:underline"
          >
            {peer.username}
          </button>
        ) : (
          <b className="truncate text-[14px]">{peer?.username ?? 'SoundCloud'}</b>
        )}
      </header>
      <MessageList withId={withId} />
      {system ? (
        <p className="flex-none px-5 py-4 text-center text-[12px] text-muted">
          {t('messages.readOnly')}
        </p>
      ) : (
        <Composer withId={withId} />
      )}
    </>
  );
}

function dayLabel(iso: string, language: string, t: (key: string) => string): string {
  const day = new Date(iso);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (day.toDateString() === today.toDateString()) return t('messages.today');
  if (day.toDateString() === yesterday.toDateString()) return t('messages.yesterday');
  return day.toLocaleDateString(language, {
    day: 'numeric',
    month: 'long',
    year: day.getFullYear() === today.getFullYear() ? undefined : 'numeric',
  });
}

/**
 * Entries oldest first with day separators. Message keys count repeats from
 * the newest end, so loading older pages doesn't change existing keys.
 */
function entries(
  newestFirst: Message[],
  pending: string | null,
  language: string,
  t: (key: string) => string,
): Entry[] {
  const seen = new Map<string, number>();
  const keyed = newestFirst.map((message) => {
    const base = `${message.sentAt}|${message.sender?.id ?? 's'}|${message.body.length}`;
    const repeat = seen.get(base) ?? 0;
    seen.set(base, repeat + 1);
    return { key: `${base}|${repeat}`, message };
  });
  const out: Entry[] = [];
  let day = '';
  for (const { key, message } of keyed.reverse()) {
    const date = new Date(message.sentAt).toDateString();
    if (date !== day) {
      day = date;
      out.push({ kind: 'day', key: `day|${date}`, label: dayLabel(message.sentAt, language, t) });
    }
    out.push({ kind: 'message', key, message });
  }
  if (pending !== null) out.push({ kind: 'pending', key: 'pending', body: pending });
  return out;
}

function MessageList({ withId }: { withId: string }) {
  const { t, i18n } = useTranslation();
  const query = useMessages(withId);
  const pending = usePendingMessage(withId);
  const messages = useMemo(
    () => query.data?.pages.flatMap((page) => page.collection) ?? [],
    [query.data],
  );
  const items = useMemo(
    () => entries(messages, pending, i18n.language, t),
    [messages, pending, i18n.language, t],
  );
  const scroller = useRef<HTMLDivElement>(null);
  const rows = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ESTIMATE,
    getItemKey: (index) => items[index].key,
    overscan: 6,
  });

  // Keep the view in place when older messages arrive above it, and follow
  // new ones at the bottom if the view was there.
  const seen = useRef({ oldest: '', newest: '', total: 0, atBottom: true });
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element || items.length === 0) return;
    const oldest = items[0].key;
    const newest = items[items.length - 1].key;
    const total = rows.getTotalSize();
    const before = seen.current;
    if (before.newest === '' || (newest !== before.newest && before.atBottom)) {
      rows.scrollToIndex(items.length - 1, { align: 'end' });
    } else if (oldest !== before.oldest && newest === before.newest) {
      rows.scrollToOffset(element.scrollTop + total - before.total);
    }
    seen.current = { ...before, oldest, newest, total };
  }, [items, rows]);

  const onScroll = () => {
    const element = scroller.current;
    if (!element) return;
    seen.current.atBottom =
      element.scrollHeight - element.scrollTop - element.clientHeight < BOTTOM_SLACK;
    if (element.scrollTop < LOAD_OLDER_AT && query.hasNextPage && !query.isFetchingNextPage) {
      void query.fetchNextPage();
    }
  };

  if (query.isError) {
    return (
      <p className="m-auto text-[12px] text-muted">
        {t('common.loadFailed')} ·{' '}
        <button
          type="button"
          onClick={() => void query.refetch()}
          className="text-accent hover:underline"
        >
          {t('common.retry')}
        </button>
      </p>
    );
  }

  return (
    <div ref={scroller} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto px-5 py-3">
      <div className="relative" style={{ height: rows.getTotalSize() }}>
        {rows.getVirtualItems().map((row) => (
          <div
            key={row.key}
            data-index={row.index}
            ref={rows.measureElement}
            className="absolute left-0 w-full"
            style={{ transform: `translateY(${row.start}px)` }}
          >
            <EntryView entry={items[row.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}

function EntryView({ entry }: { entry: Entry }) {
  const { i18n } = useTranslation();
  if (entry.kind === 'day') {
    return (
      <div className="py-3 text-center text-[11px] font-semibold text-muted">{entry.label}</div>
    );
  }
  if (entry.kind === 'pending') {
    return (
      <div className="flex justify-end py-1 opacity-60">
        <Bubble body={entry.body} mine time="" />
      </div>
    );
  }
  const { message } = entry;
  const time = new Date(message.sentAt).toLocaleTimeString(i18n.language, {
    hour: '2-digit',
    minute: '2-digit',
  });
  return (
    <div className={`flex flex-col gap-1.5 py-1 ${message.mine ? 'items-end' : 'items-start'}`}>
      <Bubble body={message.body} mine={message.mine} time={time} />
      {trackLinks(message.body).map((url) => (
        <LinkedTrack key={url} url={url} />
      ))}
    </div>
  );
}

function Bubble({ body, mine, time }: { body: string; mine: boolean; time: string }) {
  return (
    <div
      className={`max-w-[70%] rounded-2xl px-3.5 py-2 text-[13px] leading-snug ${
        mine ? 'rounded-br-md bg-accent text-on-accent' : 'rounded-bl-md bg-bg'
      }`}
    >
      <span className="whitespace-pre-wrap [overflow-wrap:anywhere]">
        <RichText text={body.trim()} linkClass={mine ? 'underline' : 'text-accent'} />
      </span>
      {time && (
        <span className={`ml-2 align-bottom text-[10px] ${mine ? 'opacity-75' : 'text-muted'}`}>
          {time}
        </span>
      )}
    </div>
  );
}

function trackLinks(body: string): string[] {
  const urls = [...body.matchAll(TRACK_LINK)].map((match) => match[0].replace(TRAILING, ''));
  return [...new Set(urls)].slice(0, MAX_PREVIEWS);
}

/** A soundcloud.com link as a playable track card; nothing if it isn't a track. */
function LinkedTrack({ url }: { url: string }) {
  const go = useRouter((state) => state.go);
  const { data: track } = useLinkedTrack(url);
  if (!track) return null;
  const blocked = track.access === 'blocked';
  return (
    <div
      className={`flex w-[280px] max-w-[70%] items-center gap-2.5 rounded-xl bg-bg p-2 ${
        blocked ? 'opacity-45' : ''
      }`}
    >
      <button
        type="button"
        disabled={blocked}
        onClick={() => void playTracks([track.id], 0)}
        className="group relative flex-none"
      >
        <Cover
          url={track.artworkUrl ?? track.user?.avatarUrl}
          size="t67x67"
          className="h-10 w-10 rounded-md"
        />
        <span className="absolute inset-0 grid place-items-center rounded-md bg-shade text-on-accent opacity-0 group-hover:opacity-100">
          <Play size={14} fill="currentColor" />
        </span>
      </button>
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={() => go({ name: 'track', id: track.id })}
          title={track.title}
          className="block max-w-full truncate text-left text-xs font-semibold hover:underline"
        >
          {track.title}
        </button>
        <div className="truncate text-[11px] text-muted">{track.user?.username}</div>
      </div>
    </div>
  );
}

function Composer({ withId }: { withId: string }) {
  const { t } = useTranslation();
  const send = useSendMessage(withId);
  const [text, setText] = useState('');
  const input = useRef<HTMLTextAreaElement>(null);

  // Grow with the text up to a few lines.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measures on each edit.
  useLayoutEffect(() => {
    const element = input.current;
    if (!element) return;
    element.style.height = 'auto';
    element.style.height = `${Math.min(element.scrollHeight, MAX_INPUT_HEIGHT)}px`;
  }, [text]);

  const submit = () => {
    const body = text.trim();
    if (!body || send.isPending) return;
    send.mutate(body, { onSuccess: () => setText('') });
  };

  return (
    <div className="flex-none border-t border-line px-5 py-3">
      <div className="flex items-end gap-2">
        <textarea
          ref={input}
          rows={1}
          value={text}
          maxLength={4000}
          disabled={send.isPending}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              submit();
            }
          }}
          placeholder={t('messages.placeholder')}
          className="min-h-9 min-w-0 flex-1 resize-none rounded-2xl bg-bg px-4 py-2 text-[13px] outline-none placeholder:text-muted focus:ring-1 focus:ring-accent disabled:opacity-60"
        />
        <button
          type="button"
          title={t('messages.send')}
          disabled={!text.trim() || send.isPending}
          onClick={submit}
          className="grid h-9 w-9 flex-none place-items-center rounded-full bg-accent text-on-accent disabled:opacity-40"
        >
          <SendHorizontal size={16} />
        </button>
      </div>
      {send.isError && <p className="mt-1.5 text-[11px] text-danger">{t('messages.sendFailed')}</p>}
    </div>
  );
}
