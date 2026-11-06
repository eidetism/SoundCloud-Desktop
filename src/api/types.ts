import type { Track, User } from '../player/api';

export type { Track, User };

export interface Page<T> {
  collection: T[];
  nextHref: string | null;
}

export interface Playlist {
  id: number;
  title: string;
  permalinkUrl: string;
  artworkUrl: string | null;
  user: User | null;
  trackCount: number;
  durationMs: number;
  isAlbum: boolean;
  likesCount: number | null;
  displayDate: string | null;
  /** Needed to open a private playlist. */
  secretToken: string | null;
  trackIds: number[];
}

/** A generated playlist: Your Mix, Weekly Wave, Daily Drops, stations… */
export interface SystemPlaylist {
  kind: 'system-playlist';
  urn: string;
  title: string;
  shortTitle: string | null;
  shortDescription: string | null;
  permalinkUrl: string;
  calculatedArtworkUrl: string | null;
  trackingFeatureName: string | null;
  trackIds: number[];
}

export type SelectionItem =
  | SystemPlaylist
  | (Playlist & { kind: 'playlist' })
  | (User & { kind: 'user' });

export interface Selection {
  urn: string;
  title: string | null;
  trackingFeatureName: string | null;
  items: Page<SelectionItem>;
}

export interface FeedItem {
  kind: 'track' | 'track-repost' | 'playlist' | 'playlist-repost';
  createdAt: string;
  user: User;
  caption: string | null;
  track: Track | null;
  playlist: Playlist | null;
}

export interface PlayHistoryItem {
  playedAt: number;
  track: Track;
}

export interface TrackLike {
  createdAt: string;
  track: Track;
}

export type SearchItem =
  | (Track & { kind: 'track' })
  | (Playlist & { kind: 'playlist' })
  | (User & { kind: 'user' });

export interface SearchPage extends Page<SearchItem> {
  totalResults: number;
}

export interface ArtistShortcut {
  user: User;
  hasRead: boolean;
}

export interface UserProfile {
  id: number;
  username: string;
  permalinkUrl: string;
  avatarUrl: string | null;
  fullName: string | null;
  city: string | null;
  description: string | null;
  verified: boolean;
  followersCount: number | null;
  followingsCount: number | null;
  trackCount: number | null;
  playlistCount: number | null;
  bannerUrl: string | null;
}

/** An entry of `/me/library/all`. */
export type LibraryItem =
  | { type: 'playlist'; playlist: Playlist }
  | { type: 'playlist-like'; playlist: Playlist }
  | { type: 'system-playlist-like'; systemPlaylist: SystemPlaylist };
