import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect } from 'react';
import type { Track } from '../api/types';

export interface OfflineIds {
  saved: number[];
  downloading: number[];
}

export interface OfflineSummary {
  folder: string;
  defaultFolder: boolean;
  savedBytes: number;
  savedCount: number;
  recentBytes: number;
  recentCount: number;
  recentLimitMb: number;
}

interface SavedTrack {
  track: Track;
  bytes: number;
  addedAt: number;
}

const IDS = ['offline-ids'];
const SAVED = ['offline-saved'];
const SUMMARY = ['offline-summary'];

/** Saved and downloading track ids; kept current by `useOfflineFeed`. */
export const useOfflineIds = () =>
  useQuery({
    queryKey: IDS,
    queryFn: () => invoke<OfflineIds>('offline_ids'),
    staleTime: Number.POSITIVE_INFINITY,
  });

/** Follow `offline:changed`. Mounted once, in the shell. */
export function useOfflineFeed() {
  const client = useQueryClient();
  useEffect(() => {
    const stop = listen<OfflineIds>('offline:changed', (event) => {
      client.setQueryData(IDS, event.payload);
      void client.invalidateQueries({ queryKey: SAVED });
      void client.invalidateQueries({ queryKey: SUMMARY });
    });
    return () => void stop.then((unlisten) => unlisten());
  }, [client]);
}

/** Saved tracks, newest first. They play from disk, so they count as playable. */
export const useSavedTracks = () =>
  useQuery({
    queryKey: SAVED,
    queryFn: () => invoke<SavedTrack[]>('offline_saved'),
    select: (saved) => saved.map(({ track }): Track => ({ ...track, access: 'full' })),
  });

export const useOfflineSummary = () =>
  useQuery({ queryKey: SUMMARY, queryFn: () => invoke<OfflineSummary>('offline_summary') });

/** Save (`saved: true`) or un-save a track. Errors: `drm`, `preview`, `unavailable`… */
export function useSetOffline() {
  return useMutation({
    mutationFn: ({ id, saved }: { id: number; saved: boolean }) =>
      invoke<void>(saved ? 'offline_save' : 'offline_remove', { id }),
  });
}

function useSummaryMutation<V>(run: (vars: V) => Promise<OfflineSummary>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSuccess: (summary) => client.setQueryData(SUMMARY, summary),
  });
}

export const useSetRecentLimit = () =>
  useSummaryMutation((megabytes: number) =>
    invoke<OfflineSummary>('offline_set_recent_limit', { megabytes }),
  );

export const useClearRecent = () =>
  useSummaryMutation(() => invoke<OfflineSummary>('offline_clear_recent'));

/**
 * Move the cache: pick a folder (`'pick'`) or go back to the default (`null`).
 * Resolves to `null` when the dialog was cancelled.
 */
export const useRelocate = () => {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (target: 'pick' | null) => {
      const parent = target === 'pick' ? await invoke<string | null>('offline_pick_folder') : null;
      if (target === 'pick' && parent === null) return null;
      return invoke<OfflineSummary>('offline_relocate', { parent });
    },
    onSuccess: (summary) => {
      if (summary) client.setQueryData(SUMMARY, summary);
    },
  });
};

/** `[done, total]` bytes of a folder move in progress. */
export function onRelocateProgress(handler: (done: number, total: number) => void) {
  return listen<[number, number]>('offline:relocate', (event) => handler(...event.payload));
}

export function formatBytes(bytes: number, language: string): string {
  const units = language.startsWith('ru') ? ['Б', 'КБ', 'МБ', 'ГБ'] : ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit >= 2 && value < 10 ? 1 : 0;
  return `${value.toLocaleString(language, { maximumFractionDigits: digits })} ${units[unit]}`;
}
