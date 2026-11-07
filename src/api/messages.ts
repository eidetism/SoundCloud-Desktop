import {
  type InfiniteData,
  useInfiniteQuery,
  useMutation,
  useMutationState,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import { create } from 'zustand';
import type { Page, Track, User } from './types';

export interface Message {
  body: string;
  sender: User | null;
  mine: boolean;
  system: boolean;
  sentAt: string;
}

export interface Conversation {
  /** The other side's user id, or `system`. */
  with: string;
  peer: User | null;
  system: boolean;
  read: boolean;
  lastMessage: Message | null;
}

const UNREAD_EVENT = 'messages:unread';

export const useConversations = () =>
  useInfiniteQuery<
    Page<Conversation>,
    Error,
    InfiniteData<Page<Conversation>>,
    unknown[],
    string | null
  >({
    queryKey: ['conversations'],
    queryFn: ({ pageParam }) => invoke('api_conversations', { cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextHref ?? undefined,
  });

export const useConversation = (withId: string) =>
  useQuery({
    queryKey: ['conversation', withId],
    queryFn: () => invoke<Conversation>('api_conversation', { with: withId }),
  });

/** Messages of a conversation; pages go back in time (newest first). */
export const useMessages = (withId: string) =>
  useInfiniteQuery<Page<Message>, Error, InfiniteData<Page<Message>>, unknown[], string | null>({
    queryKey: ['messages', withId],
    queryFn: ({ pageParam }) => invoke('api_messages', { with: withId, cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextHref ?? undefined,
  });

function useRefreshMessages() {
  const client = useQueryClient();
  return (withId: string) => {
    void client.invalidateQueries({ queryKey: ['conversations'] });
    void client.invalidateQueries({ queryKey: ['conversation', withId] });
    void client.invalidateQueries({ queryKey: ['messages', withId] });
  };
}

export function useMarkRead() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (withId: string) => invoke<void>('api_conversation_read', { with: withId }),
    onSuccess: (_result, withId) => {
      client.setQueryData<Conversation>(['conversation', withId], (old) =>
        old ? { ...old, read: true } : old,
      );
      void client.invalidateQueries({ queryKey: ['conversations'] });
    },
  });
}

export function useSendMessage(withId: string) {
  const refresh = useRefreshMessages();
  return useMutation({
    mutationKey: ['send-message', withId],
    mutationFn: (body: string) => invoke<Conversation>('api_message_send', { with: withId, body }),
    onSuccess: () => refresh(withId),
  });
}

/** The text being sent in a conversation, if any. */
export function usePendingMessage(withId: string): string | null {
  const pending = useMutationState({
    filters: { mutationKey: ['send-message', withId], status: 'pending' },
    select: (mutation) => mutation.state.variables as string,
  });
  return pending[pending.length - 1] ?? null;
}

/** The track behind a soundcloud.com track link (`null` for anything else). */
export const useLinkedTrack = (url: string) =>
  useQuery({
    queryKey: ['resolve-track', url],
    queryFn: () => invoke<Track | null>('api_resolve_track', { url }),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });

/** Unread conversations, as Rust last counted them. */
export const useUnread = create<{ count: number }>(() => ({ count: 0 }));

/**
 * Feed `useUnread` from Rust and refetch open message lists when it reports
 * a change. Mounted once, in the shell.
 */
export function useUnreadFeed() {
  const client = useQueryClient();
  useEffect(() => {
    let alive = true;
    void invoke<number>('messages_unread').then((count) => alive && useUnread.setState({ count }));
    const stop = listen<number>(UNREAD_EVENT, (event) => {
      useUnread.setState({ count: event.payload });
      void client.invalidateQueries({ queryKey: ['conversations'] });
      void client.invalidateQueries({ queryKey: ['conversation'] });
      void client.invalidateQueries({ queryKey: ['messages'] });
    });
    return () => {
      alive = false;
      void stop.then((unlisten) => unlisten());
    };
  }, [client]);
}
