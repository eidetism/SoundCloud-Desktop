import {
  Compass,
  Disc3,
  Download,
  Heart,
  History,
  House,
  ListMusic,
  type LucideIcon,
  MessageSquare,
  MicVocal,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AppLogo } from '../components/AppLogo';
import { type Section, useRouter } from './router';
import { UnreadBadge } from './UnreadBadge';

const MENU: [Section, LucideIcon][] = [
  ['home', House],
  ['discover', Compass],
  ['albums', Disc3],
  ['artists', MicVocal],
  ['messages', MessageSquare],
];

const LIBRARY: [Section, LucideIcon][] = [
  ['likes', Heart],
  ['history', History],
  ['playlists', ListMusic],
  ['offline', Download],
];

function Logo() {
  return (
    <div data-tauri-drag-region className="mx-6 mb-[22px] w-fit">
      <AppLogo size={52} />
    </div>
  );
}

function NavGroup({ title, items }: { title: string; items: [Section, LucideIcon][] }) {
  const { t } = useTranslation();
  const { route, go } = useRouter();
  return (
    <>
      <div className="px-6 pb-2.5 text-[13px] font-semibold text-side-hi">{title}</div>
      <nav className="mb-[18px]">
        {items.map(([section, Icon]) => {
          const on = route.name === section || (section === 'messages' && route.name === 'chat');
          return (
            <button
              key={section}
              type="button"
              onClick={() => go({ name: section })}
              className={`relative flex w-full items-center gap-3 px-6 py-[9px] text-left transition-colors ${
                on
                  ? 'bg-[linear-gradient(90deg,color-mix(in_srgb,var(--accent)_18%,transparent),transparent)] text-side-hi'
                  : 'hover:text-side-hi'
              }`}
            >
              {on && (
                <span className="absolute top-1.5 bottom-1.5 left-0 w-[3px] rounded-r-[3px] bg-accent" />
              )}
              <Icon size={16} strokeWidth={2} />
              {t(`nav.${section}`)}
              {section === 'messages' && <UnreadBadge className="ml-auto" />}
            </button>
          );
        })}
      </nav>
    </>
  );
}

export function Sidebar({ username }: { username: string }) {
  const { t } = useTranslation();
  return (
    <aside className="flex w-60 flex-none flex-col overflow-hidden pt-6 text-[13px] text-side-text">
      <Logo />
      <div className="px-6 text-xs">{t('shell.hi')}</div>
      <div className="truncate px-6 pt-1 pb-[26px] text-xl leading-tight font-bold text-side-hi">
        {username}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <NavGroup title={t('shell.menu')} items={MENU} />
        <NavGroup title={t('shell.library')} items={LIBRARY} />
      </div>
    </aside>
  );
}
