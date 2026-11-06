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
  | { name: 'artist'; id: number };

const BACK_LIMIT = 50;

interface Router {
  route: Route;
  /** Earlier routes, newest last, for "back". */
  past: Route[];
  go: (route: Route) => void;
  back: () => void;
}

/** Current screen. Kept in the frontend for now; phase 9 restores it from Rust. */
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
