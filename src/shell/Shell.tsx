import { useLayoutEffect, useRef } from 'react';
import { useUnreadFeed } from '../api/messages';
import type { Me } from '../auth/api';
import { useOfflineFeed } from '../offline/api';
import { OfflineScreen } from '../offline/OfflineScreen';
import type { PlayerSnapshot } from '../player/api';
import { PlayerBar } from '../player/PlayerBar';
import { usePlayer } from '../player/usePlayer';
import { useSpaceToggle } from '../player/useSpaceToggle';
import { ArtistScreen, ArtistsScreen } from '../screens/Artist';
import { MixScreen, PlaylistScreen } from '../screens/Collection';
import { HomeScreen } from '../screens/Home';
import { LibraryScreen } from '../screens/Library';
import { DiscoverScreen, FeedScreen, HistoryScreen, LikesScreen } from '../screens/Lists';
import { MessagesScreen } from '../screens/messages/MessagesScreen';
import { SearchScreen } from '../screens/Search';
import { TrackScreen } from '../screens/track/TrackScreen';
import { SettingsScreen } from '../settings/SettingsScreen';
import { useRouter, useRouteSync } from './router';
import { Sidebar } from './Sidebar';
import { ScrollRoot } from './scroll';
import { TopBar } from './TopBar';

function Screen({ player, user }: { player: PlayerSnapshot | null; user: Me | null }) {
  const route = useRouter((state) => state.route);
  switch (route.name) {
    case 'home':
      return <HomeScreen player={player} />;
    case 'discover':
      return <DiscoverScreen />;
    case 'feed':
      return <FeedScreen player={player} />;
    case 'likes':
      return <LikesScreen player={player} />;
    case 'history':
      return <HistoryScreen player={player} />;
    case 'artists':
      return <ArtistsScreen />;
    case 'search':
      return <SearchScreen key={route.query} query={route.query} player={player} />;
    case 'track':
      return <TrackScreen key={route.id} id={route.id} player={player} me={user} />;
    case 'playlist':
      return (
        <PlaylistScreen
          key={route.id}
          id={route.id}
          secretToken={route.secretToken}
          player={player}
        />
      );
    case 'mix':
      return <MixScreen key={route.mix.urn} mix={route.mix} player={player} />;
    case 'artist':
      return <ArtistScreen key={route.id} id={route.id} player={player} />;
    case 'settings':
      return <SettingsScreen />;
    case 'albums':
      return <LibraryScreen albumsOnly />;
    case 'messages':
      return <MessagesScreen withId={null} />;
    case 'chat':
      return <MessagesScreen withId={route.with} />;
    case 'playlists':
      return <LibraryScreen />;
    case 'offline':
      return <OfflineScreen player={player} />;
  }
}

/** The signed-in window: dark frame, sidebar and content panel, the player bar under both. */
export function Shell({ user }: { user: Me | null }) {
  const player = usePlayer();
  const route = useRouter((state) => state.route);
  const scroller = useRef<HTMLDivElement>(null);
  useSpaceToggle();
  useUnreadFeed();
  useOfflineFeed();
  useRouteSync();
  // Messages scroll their own panes; every other screen scrolls the panel.
  const fills = route.name === 'messages' || route.name === 'chat';

  // Every screen starts at the top.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs on each route change.
  useLayoutEffect(() => {
    scroller.current?.scrollTo(0, 0);
  }, [route]);

  return (
    <div className="flex h-full flex-col gap-3 bg-frame">
      <div className="flex min-h-0 flex-1 pt-3 pr-3">
        <Sidebar username={user?.username ?? '…'} />
        <main className="flex min-w-0 flex-1 flex-col gap-4 rounded-xl bg-bg px-[26px] py-[22px]">
          <TopBar user={user} />
          <ScrollRoot.Provider value={scroller}>
            {fills ? (
              <div className="min-h-0 flex-1">
                <Screen player={player} user={user} />
              </div>
            ) : (
              <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
                <div>
                  <Screen player={player} user={user} />
                </div>
              </div>
            )}
          </ScrollRoot.Provider>
        </main>
      </div>
      {player && <PlayerBar player={player} />}
    </div>
  );
}
