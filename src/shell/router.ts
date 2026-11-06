import { create } from 'zustand';

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
  | 'settings';

export type Route = { name: Section } | { name: 'search'; query: string };

interface Router {
  route: Route;
  go: (route: Route) => void;
}

/** Current screen. Kept in the frontend for now; phase 9 restores it from Rust. */
export const useRouter = create<Router>((set) => ({
  route: { name: 'home' },
  go: (route) => set({ route }),
}));
