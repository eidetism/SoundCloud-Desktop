import {
  Compass,
  Disc3,
  Download,
  History,
  House,
  ListMusic,
  MessageSquare,
  MicVocal,
  Search,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { Me } from '../auth/api';
import { LikesList } from '../library/LikesList';
import type { PlayerSnapshot } from '../player/api';
import { usePlayer } from '../player/usePlayer';
import { useSpaceToggle } from '../player/useSpaceToggle';
import { SettingsScreen } from '../settings/SettingsScreen';
import { Placeholder } from './Placeholder';
import { useRouter } from './router';
import { Sidebar } from './Sidebar';
import { TopBar } from './TopBar';

function Screen({ player }: { player: PlayerSnapshot | null }) {
  const { t } = useTranslation();
  const route = useRouter((state) => state.route);
  switch (route.name) {
    case 'likes':
      return <LikesList player={player} />;
    case 'settings':
      return <SettingsScreen />;
    case 'search':
      return <Placeholder title={`${t('shell.search')}: ${route.query}`} icon={Search} />;
    case 'home':
      return <Placeholder title={t('nav.home')} icon={House} />;
    case 'discover':
      return <Placeholder title={t('nav.discover')} icon={Compass} />;
    case 'albums':
      return <Placeholder title={t('nav.albums')} icon={Disc3} />;
    case 'artists':
      return <Placeholder title={t('nav.artists')} icon={MicVocal} />;
    case 'messages':
      return <Placeholder title={t('nav.messages')} icon={MessageSquare} />;
    case 'history':
      return <Placeholder title={t('nav.history')} icon={History} />;
    case 'playlists':
      return <Placeholder title={t('nav.playlists')} icon={ListMusic} />;
    case 'offline':
      return <Placeholder title={t('nav.offline')} icon={Download} />;
  }
}

/** The signed-in window: dark frame, sidebar with the player, light/dark content panel. */
export function Shell({ user }: { user: Me | null }) {
  const player = usePlayer();
  useSpaceToggle();
  return (
    <div className="flex h-full bg-frame py-3 pr-3">
      <Sidebar username={user?.username ?? '…'} player={player} />
      <main className="flex min-w-0 flex-1 flex-col gap-4 rounded-xl bg-bg px-[26px] py-[22px]">
        <TopBar user={user} />
        <div className="min-h-0 flex-1 overflow-y-auto">
          <Screen player={player} />
        </div>
      </main>
    </div>
  );
}
