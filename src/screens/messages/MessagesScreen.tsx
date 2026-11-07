import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { type Conversation, useConversations } from '../../api/messages';
import { flatten, nextPage } from '../../api/queries';
import { Cover } from '../../components/Cover';
import { useRouter } from '../../shell/router';
import { timeAgo } from '../track/text';
import { Chat } from './Chat';

const ROW_HEIGHT = 64;

/** Conversations on the left, the open one on the right. */
export function MessagesScreen({ withId }: { withId: string | null }) {
  const { t } = useTranslation();
  return (
    <div className="grid h-full grid-cols-[300px_minmax(0,1fr)] gap-4">
      <section className="flex min-h-0 flex-col">
        <h1 className="mb-3 text-[22px] font-bold">{t('messages.title')}</h1>
        <ConversationList selected={withId} />
      </section>
      <section className="flex min-h-0 flex-col rounded-xl bg-card">
        {withId ? (
          <Chat key={withId} withId={withId} />
        ) : (
          <p className="m-auto text-[13px] text-muted">{t('messages.pick')}</p>
        )}
      </section>
    </div>
  );
}

function ConversationList({ selected }: { selected: string | null }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const query = useConversations();
  const items = flatten(query.data);
  const scroller = useRef<HTMLDivElement>(null);
  const rows = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    getItemKey: (index) => items[index].with,
    overscan: 6,
  });
  const visible = rows.getVirtualItems();
  const last = visible[visible.length - 1]?.index ?? -1;
  // biome-ignore lint/correctness/useExhaustiveDependencies: `query` changes every render.
  useEffect(() => {
    if (items.length > 0 && last >= items.length - 3) nextPage(query)();
  }, [last, items.length]);

  if (query.isLoading) {
    return (
      <div className="flex flex-col gap-1">
        {Array.from({ length: 6 }, (_, i) => (
          <div key={i} className="flex h-16 items-center gap-3 px-2">
            <div className="h-10 w-10 flex-none rounded-full bg-card" />
            <div className="flex-1">
              <div className="h-2.5 w-2/3 rounded bg-card" />
              <div className="mt-2 h-2 w-full rounded bg-card" />
            </div>
          </div>
        ))}
      </div>
    );
  }
  if (query.isError) {
    return (
      <p className="text-[12px] text-muted">
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
  if (items.length === 0) return <p className="text-[12px] text-muted">{t('messages.empty')}</p>;

  return (
    <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
      <div className="relative" style={{ height: rows.getTotalSize() }}>
        {rows.getVirtualItems().map((row) => {
          const conversation = items[row.index];
          return (
            <div
              key={row.key}
              className="absolute left-0 w-full"
              style={{ height: row.size, transform: `translateY(${row.start}px)` }}
            >
              <ConversationRow
                conversation={conversation}
                selected={conversation.with === selected}
                onOpen={() => go({ name: 'chat', with: conversation.with })}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function ConversationRow({
  conversation,
  selected,
  onOpen,
}: {
  conversation: Conversation;
  selected: boolean;
  onOpen: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { peer, lastMessage } = conversation;
  const unread = !conversation.read && lastMessage !== null && !lastMessage.mine;
  const preview = lastMessage?.body.replace(/\s+/g, ' ').trim() ?? '';
  return (
    <button
      type="button"
      onClick={onOpen}
      className={`flex h-full w-full items-center gap-3 rounded-lg px-2 text-left ${
        selected ? 'bg-card' : 'hover:bg-hover'
      }`}
    >
      <Cover url={peer?.avatarUrl} size="t67x67" className="h-10 w-10 flex-none rounded-full" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className={`truncate text-[13px] ${unread ? 'font-bold' : 'font-semibold'}`}>
            {peer?.username ?? 'SoundCloud'}
          </span>
          {lastMessage && (
            <span className="ml-auto flex-none text-[10px] text-muted">
              {timeAgo(lastMessage.sentAt, i18n.language)}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <span className={`truncate text-[12px] ${unread ? 'text-text' : 'text-muted'}`}>
            {lastMessage?.mine && `${t('messages.you')} `}
            {preview}
          </span>
          {unread && <span className="ml-auto h-2 w-2 flex-none rounded-full bg-accent" />}
        </div>
      </div>
    </button>
  );
}
