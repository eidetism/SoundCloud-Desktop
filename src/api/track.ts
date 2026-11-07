import {
  type InfiniteData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { Comment, CommentPage, Page, Playlist, Track, User } from './types';

/* Queries and writes of the track page. Writes go through Rust's hidden site window. */

type Comments = InfiniteData<CommentPage, string | null>;
type Playlists = InfiniteData<Page<Playlist>, string | null>;

export const useComments = (trackUrn: string, creatorUrn: string, enabled = true) =>
  useInfiniteQuery<CommentPage, Error, Comments, unknown[], string | null>({
    queryKey: ['comments', trackUrn],
    queryFn: ({ pageParam }) =>
      invoke<CommentPage>('api_comments', { trackUrn, creatorUrn, cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.cursor ?? undefined,
    enabled,
  });

export const useRelated = (id: number) =>
  useQuery({
    queryKey: ['related', id],
    queryFn: () => invoke<Page<Track>>('api_related', { id }),
    select: (page) => page.collection,
  });

/** Playlists (`albums: false`) or albums containing the track. */
export const useTrackSets = (id: number, albums: boolean) =>
  useInfiniteQuery<Page<Playlist>, Error, Playlists, unknown[], string | null>({
    queryKey: ['track-sets', id, albums],
    queryFn: ({ pageParam }) =>
      invoke<Page<Playlist>>('api_track_sets', { id, albums, cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextHref ?? undefined,
  });

export const useLikers = (id: number) =>
  useQuery({
    queryKey: ['likers', id],
    queryFn: () => invoke<Page<User>>('api_likers', { id, cursor: null }),
    select: (page) => page.collection,
  });

const idSet = (ids: number[]) => new Set(ids);

export const useRepostedIds = () =>
  useQuery({
    queryKey: ['reposted-ids'],
    queryFn: () => invoke<number[]>('api_reposted_ids'),
    select: idSet,
    staleTime: Number.POSITIVE_INFINITY,
  });

export const useFollowingIds = () =>
  useQuery({
    queryKey: ['following-ids'],
    queryFn: () => invoke<number[]>('api_following_ids'),
    select: idSet,
    staleTime: Number.POSITIVE_INFINITY,
  });

export const useOwnPlaylists = (enabled: boolean) =>
  useInfiniteQuery<Page<Playlist>, Error, Playlists, unknown[], string | null>({
    queryKey: ['own-playlists'],
    queryFn: ({ pageParam }) => invoke<Page<Playlist>>('api_own_playlists', { cursor: pageParam }),
    initialPageParam: null,
    getNextPageParam: (last) => last.nextHref ?? undefined,
    enabled,
  });

/** Repost or un-repost; the button flips at once and flips back on failure. */
export function useSetRepost() {
  const client = useQueryClient();
  const key = ['reposted-ids'];
  return useMutation({
    mutationFn: ({ track, reposted }: { track: Track; reposted: boolean }) =>
      invoke<void>('api_set_repost', { trackUrn: track.urn, reposted }),
    onMutate: async ({ track, reposted }) => {
      await client.cancelQueries({ queryKey: key });
      const previous = client.getQueryData<number[]>(key);
      client.setQueryData<number[]>(key, (ids = []) =>
        reposted
          ? [track.id, ...ids.filter((x) => x !== track.id)]
          : ids.filter((x) => x !== track.id),
      );
      return { previous };
    },
    onError: (_error, _vars, context) => {
      if (context?.previous) client.setQueryData(key, context.previous);
    },
  });
}

/** Apply `change` to every cached comment and reply; `null` removes one. */
function mapComments(data: Comments | undefined, change: (comment: Comment) => Comment | null) {
  if (!data) return data;
  const walk = (list: Comment[]): Comment[] =>
    list.flatMap((comment) => {
      const changed = change(comment);
      return changed ? [{ ...changed, replies: walk(changed.replies) }] : [];
    });
  return { ...data, pages: data.pages.map((page) => ({ ...page, comments: walk(page.comments) })) };
}

export function useCommentLike(trackUrn: string) {
  const client = useQueryClient();
  const key = ['comments', trackUrn];
  return useMutation({
    mutationFn: ({ comment, liked }: { comment: Comment; liked: boolean }) =>
      invoke<void>('api_comment_like', { commentUrn: comment.urn, trackUrn, liked }),
    onMutate: async ({ comment, liked }) => {
      await client.cancelQueries({ queryKey: key });
      const previous = client.getQueryData<Comments>(key);
      client.setQueryData<Comments>(key, (data) =>
        mapComments(data, (c) =>
          c.urn === comment.urn && c.liked !== liked
            ? { ...c, liked, likes: Math.max(0, c.likes + (liked ? 1 : -1)) }
            : c,
        ),
      );
      return { previous };
    },
    onError: (_error, _vars, context) => {
      if (context?.previous) client.setQueryData(key, context.previous);
    },
  });
}

/** Post a comment, or a reply to `parent` (shown in its thread). */
export function useCreateComment(trackUrn: string) {
  const client = useQueryClient();
  const key = ['comments', trackUrn];
  return useMutation({
    mutationFn: ({ atMs, body }: { atMs: number; body: string; parent: Comment | null }) =>
      invoke<Comment>('api_comment_create', { trackUrn, atMs, body }),
    onSuccess: (created, { parent }) => {
      client.setQueryData<Comments>(key, (data) => {
        if (!data || data.pages.length === 0) return data;
        if (parent) {
          return mapComments(data, (c) =>
            c.urn === parent.urn
              ? { ...c, replies: [...c.replies, created], repliesTotal: c.repliesTotal + 1 }
              : c,
          );
        }
        const [first, ...rest] = data.pages;
        return { ...data, pages: [{ ...first, comments: [created, ...first.comments] }, ...rest] };
      });
    },
  });
}

export function useDeleteComment(trackUrn: string) {
  const client = useQueryClient();
  const key = ['comments', trackUrn];
  return useMutation({
    mutationFn: (comment: Comment) =>
      invoke<void>('api_comment_delete', { commentUrn: comment.urn }),
    onSuccess: (_result, comment) => {
      client.setQueryData<Comments>(key, (data) =>
        mapComments(data, (c) => {
          if (c.urn === comment.urn) return null;
          const kept = c.replies.filter((reply) => reply.urn !== comment.urn);
          return kept.length === c.replies.length
            ? c
            : { ...c, replies: kept, repliesTotal: Math.max(0, c.repliesTotal - 1) };
        }),
      );
    },
  });
}

/** Add the track to one of the user's playlists, or remove it. */
export function usePlaylistSetTrack() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (vars: { playlistId: number; trackId: number; present: boolean }) =>
      invoke<number[]>('api_playlist_set_track', vars),
    onSuccess: (trackIds, { playlistId }) => {
      client.setQueryData<Playlists>(
        ['own-playlists'],
        (data) =>
          data && {
            ...data,
            pages: data.pages.map((page) => ({
              ...page,
              collection: page.collection.map((playlist) =>
                playlist.id === playlistId
                  ? { ...playlist, trackIds, trackCount: trackIds.length }
                  : playlist,
              ),
            })),
          },
      );
      void client.invalidateQueries({ queryKey: ['playlist', playlistId] });
    },
  });
}
