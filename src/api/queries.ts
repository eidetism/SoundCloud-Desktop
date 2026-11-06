import {
  type InfiniteData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type {
  ArtistShortcut,
  FeedItem,
  Page,
  PlayHistoryItem,
  Playlist,
  SearchPage,
  Selection,
  Track,
  TrackLike,
  UserProfile,
} from './types';

/** A cursor-paged api-v2 list; the next page starts at the previous `nextHref`. */
function usePaged<T, P extends Page<T> = Page<T>>(
  key: unknown[],
  command: string,
  args: Record<string, unknown> = {},
  enabled = true,
) {
  return useInfiniteQuery<P, Error, InfiniteData<P>, unknown[], string | null>({
    queryKey: key,
    queryFn: ({ pageParam }) => invoke<P>(command, { ...args, cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextHref ?? undefined,
    enabled,
  });
}

export const useFeed = () => usePaged<FeedItem>(['feed'], 'api_feed');
export const useLikes = () => usePaged<TrackLike>(['likes'], 'api_likes');
export const useHistory = () => usePaged<PlayHistoryItem>(['history'], 'api_history');
export const useDiscover = () => usePaged<Selection>(['discover'], 'api_discover');
export const useArtistShortcuts = () =>
  usePaged<ArtistShortcut>(['artist-shortcuts'], 'api_artist_shortcuts');
export const useSearch = (query: string) =>
  usePaged<SearchPage['collection'][number], SearchPage>(
    ['search', query],
    'api_search',
    { query },
    query.length > 0,
  );

export const useUserTracks = (id: number) =>
  usePaged<Track>(['user-tracks', id], 'api_user_tracks', { id });

export const useUser = (id: number) =>
  useQuery({ queryKey: ['user', id], queryFn: () => invoke<UserProfile>('api_user', { id }) });

export const usePlaylist = (id: number | null) =>
  useQuery({
    queryKey: ['playlist', id],
    queryFn: () => invoke<Playlist>('api_playlist', { id }),
    enabled: id !== null,
  });

/** Tracks for `ids` in order (Rust fetches the unknown ones). */
export const useTracks = (ids: number[], enabled = true) =>
  useQuery({
    queryKey: ['tracks', ids],
    queryFn: () => invoke<Track[]>('api_tracks', { ids }),
    enabled: enabled && ids.length > 0,
  });

/** Ids of every liked track, as a set; loaded once per session. */
export const useLikedIds = () =>
  useQuery({
    queryKey: ['liked-ids'],
    queryFn: () => invoke<number[]>('api_liked_ids'),
    select: (ids) => new Set(ids),
    staleTime: Number.POSITIVE_INFINITY,
  });

/** All items of all loaded pages, flattened. */
export function flatten<T>(data: InfiniteData<Page<T>> | undefined): T[] {
  return data?.pages.flatMap((page) => page.collection) ?? [];
}

/**
 * Like or unlike a track. The heart flips at once; on failure it flips back.
 * The likes list refetches afterwards.
 */
export function useSetLike() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, liked }: { id: number; liked: boolean }) =>
      invoke<void>('api_set_like', { id, liked }),
    onMutate: async ({ id, liked }) => {
      await client.cancelQueries({ queryKey: ['liked-ids'] });
      const previous = client.getQueryData<number[]>(['liked-ids']);
      client.setQueryData<number[]>(['liked-ids'], (ids = []) =>
        liked ? [id, ...ids.filter((x) => x !== id)] : ids.filter((x) => x !== id),
      );
      return { previous };
    },
    onError: (_error, _vars, context) => {
      if (context?.previous) client.setQueryData(['liked-ids'], context.previous);
    },
    onSettled: () => client.invalidateQueries({ queryKey: ['likes'] }),
  });
}

/** `onEnd` for a `TrackList` over an infinite query: fetch the next page once. */
export function nextPage(query: {
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  fetchNextPage: () => unknown;
}) {
  return () => {
    if (query.hasNextPage && !query.isFetchingNextPage) void query.fetchNextPage();
  };
}
