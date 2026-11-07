import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface User {
  id: number;
  username: string;
  permalink: string;
  permalinkUrl: string;
  avatarUrl: string | null;
  verified: boolean;
  followersCount: number | null;
  trackCount: number | null;
}

export type Access = 'full' | 'preview' | 'blocked';

export interface Track {
  id: number;
  urn: string;
  title: string;
  user: User | null;
  artworkUrl: string | null;
  durationMs: number;
  permalinkUrl: string;
  waveformUrl: string | null;
  genre: string | null;
  displayDate: string | null;
  likesCount: number | null;
  repostsCount: number | null;
  playbackCount: number | null;
  commentCount: number | null;
  description: string | null;
  tags: string[];
  access: Access;
}

export type Status = 'idle' | 'loading' | 'playing' | 'paused';
export type Repeat = 'off' | 'all' | 'one';
export type PlayerError = 'unavailable' | 'network' | 'signedOut';

export interface PlayerSnapshot {
  status: Status;
  track: Track | null;
  index: number | null;
  queueLength: number;
  /** Ids of the next tracks in play order. */
  upcoming: number[];
  shuffle: boolean;
  repeat: Repeat;
  volume: number;
  preview: boolean;
  error: PlayerError | null;
}

export const getPlayerState = () => invoke<PlayerSnapshot>('player_state');
/** Replace the queue with `ids` and play `ids[start]`, from `at` seconds if given. */
export const playTracks = (ids: number[], start: number, at?: number) =>
  invoke<void>('player_play', { ids, start, at: at ?? null });
export const playNext = (ids: number[]) => invoke<void>('player_play_next', { ids });
export const appendToQueue = (ids: number[]) => invoke<void>('player_append', { ids });
export const togglePlay = () => invoke<void>('player_toggle');
export const nextTrack = () => invoke<void>('player_next');
export const previousTrack = () => invoke<void>('player_previous');
export const seek = (seconds: number) => invoke<void>('player_seek', { seconds });
export const setVolume = (percent: number) => invoke<void>('player_set_volume', { percent });
export const setShuffle = (shuffle: boolean) => invoke<void>('player_set_shuffle', { shuffle });
/** Jump `steps` tracks ahead in play order (1 = the next one). */
export const skipAhead = (steps: number) => invoke<void>('player_skip_ahead', { steps });
export const setRepeat = (repeat: Repeat) => invoke<void>('player_set_repeat', { repeat });

export const onPlayerState = (handler: (snapshot: PlayerSnapshot) => void) =>
  listen<PlayerSnapshot>('player:state', (event) => handler(event.payload));

/** Playback position in seconds, ~10×/s while the window is visible. */
export const onPosition = (handler: (seconds: number) => void) =>
  listen<number>('audio:tick', (event) => handler(event.payload));
