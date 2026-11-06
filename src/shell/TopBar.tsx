import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  ChevronLeft,
  LogOut,
  MessageSquare,
  Minus,
  Search,
  Settings,
  Square,
  X,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { logout, type Me } from '../auth/api';
import { scimg } from '../lib/scimg';
import { useRouter } from './router';

function WindowControls() {
  const { t } = useTranslation();
  const appWindow = getCurrentWindow();
  const button =
    'grid h-8 w-8 place-items-center rounded-full text-muted hover:bg-hover hover:text-text';
  return (
    <div className="flex items-center gap-0.5">
      <button
        type="button"
        title={t('app.minimize')}
        className={button}
        onClick={() => void appWindow.minimize()}
      >
        <Minus size={15} />
      </button>
      <button
        type="button"
        title={t('app.maximize')}
        className={button}
        onClick={() => void appWindow.toggleMaximize()}
      >
        <Square size={12} />
      </button>
      <button
        type="button"
        title={t('app.close')}
        className="grid h-8 w-8 place-items-center rounded-full text-muted hover:bg-accent hover:text-on-accent"
        onClick={() => void appWindow.close()}
      >
        <X size={15} />
      </button>
    </div>
  );
}

function SearchBox() {
  const { t } = useTranslation();
  const { route, go } = useRouter();
  const [query, setQuery] = useState(route.name === 'search' ? route.query : '');
  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const trimmed = query.trim();
        if (trimmed) go({ name: 'search', query: trimmed });
      }}
      className="flex h-[34px] w-44 items-center gap-2 rounded-full border border-line px-3.5 text-xs text-muted focus-within:border-accent focus-within:w-60 transition-[width]"
    >
      <Search size={13} className="flex-none" />
      <input
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder={t('shell.search')}
        className="min-w-0 flex-1 bg-transparent text-text outline-none placeholder:text-muted"
      />
    </form>
  );
}

function AccountMenu({ user }: { user: Me | null }) {
  const { t } = useTranslation();
  const go = useRouter((state) => state.go);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', close);
    return () => window.removeEventListener('mousedown', close);
  }, [open]);

  const item =
    'flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px] hover:bg-hover';
  return (
    <div ref={box} className="relative">
      <button
        type="button"
        title={user?.username}
        onClick={() => setOpen((shown) => !shown)}
        className="block h-[34px] w-[34px] overflow-hidden rounded-full bg-card"
      >
        {user?.avatarUrl && (
          <img
            src={scimg(user.avatarUrl, 't67x67')}
            alt=""
            decoding="async"
            className="h-full w-full object-cover"
          />
        )}
      </button>
      {open && (
        <div className="absolute top-10 right-0 z-20 w-48 rounded-md border border-line bg-bg p-1 shadow-lg">
          <button
            type="button"
            className={item}
            onClick={() => {
              setOpen(false);
              go({ name: 'settings' });
            }}
          >
            <Settings size={15} />
            {t('nav.settings')}
          </button>
          <button type="button" className={`${item} text-danger`} onClick={() => void logout()}>
            <LogOut size={15} />
            {t('auth.signOut')}
          </button>
        </div>
      )}
    </div>
  );
}

/** Search, messages, account and window controls; the empty part drags the window. */
export function TopBar({ user }: { user: Me | null }) {
  const { t } = useTranslation();
  const { go, back, past } = useRouter();
  return (
    <header
      data-tauri-drag-region
      className="flex h-[34px] flex-none items-center justify-end gap-3"
    >
      {past.length > 0 && (
        <button
          type="button"
          title={t('shell.back')}
          onClick={back}
          className="mr-auto grid h-[34px] w-[34px] place-items-center rounded-full text-muted hover:bg-hover hover:text-text"
        >
          <ChevronLeft size={18} />
        </button>
      )}
      <SearchBox />
      <button
        type="button"
        title={t('nav.messages')}
        onClick={() => go({ name: 'messages' })}
        className="grid h-[34px] w-[34px] place-items-center rounded-full bg-accent text-on-accent"
      >
        <MessageSquare size={16} />
      </button>
      <AccountMenu user={user} />
      <WindowControls />
    </header>
  );
}
