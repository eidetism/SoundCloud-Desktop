import { invoke } from '@tauri-apps/api/core';
import { useEffect } from 'react';
import { create } from 'zustand';
import type { SystemPlaylist } from '../api/types';

export type Section =
  | 'home'
  | 'discover'
  | 'albums'
  | 'artists'
  | 'messages'
  | 'likes'
  | 'history'
  | 'playlists'
  | 'offline'
  | 'settings'
  | 'feed';

export type Route =
  | { name: Section }
  | { name: 'search'; query: string }
  | { name: 'track'; id: number }
  | { name: 'playlist'; id: number; secretToken: string | null }
  | { name: 'mix'; mix: SystemPlaylist }
  | { name: 'artist'; id: number }
  | { name: 'chat'; with: string };

const BACK_LIMIT = 50;

interface Router {
  route: Route;
  /** Earlier routes, newest last, for "back". */
  past: Route[];
  go: (route: Route) => void;
  back: () => void;
}

/** Current screen; Rust keeps a copy so a recreated window reopens it (`useRouteSync`). */
export const useRouter = create<Router>((set) => ({
  route: { name: 'home' },
  past: [],
  go: (route) => set((state) => ({ route, past: [...state.past, state.route].slice(-BACK_LIMIT) })),
  back: () =>
    set((state) =>
      state.past.length === 0
        ? state
        : { route: state.past[state.past.length - 1], past: state.past.slice(0, -1) },
    ),
}));

type Saved = Pick<Router, 'route' | 'past'>;

/**
 * Reopen the screen Rust kept from before the window was closed to the tray,
 * then report every change back. Mounted once, in the shell.
 */
export function useRouteSync() {
  useEffect(() => {
    let ready = false;
    void invoke<Saved | null>('screen_get')
      .then((saved) => {
        if (saved) useRouter.setState({ route: saved.route, past: saved.past });
      })
      .finally(() => {
        ready = true;
      });
    return useRouter.subscribe((state) => {
      if (ready) void invoke('screen_set', { screen: { route: state.route, past: state.past } });
    });
  }, []);
}
