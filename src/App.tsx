import { getCurrentWindow } from '@tauri-apps/api/window';
import { Minus, X } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { LoginScreen } from './auth/LoginScreen';
import { useSession } from './auth/useSession';
import { applyTheme, getUiSettings } from './settings/api';
import { Shell } from './shell/Shell';

/** Signed out: only the login screen, in the same frame. */
function SignedOut() {
  const { t } = useTranslation();
  const appWindow = getCurrentWindow();
  const button =
    'grid h-8 w-8 place-items-center rounded-full text-muted hover:bg-hover hover:text-text';
  return (
    <div className="flex h-full flex-col bg-frame p-3">
      <main className="flex flex-1 flex-col rounded-xl bg-bg p-4">
        <header data-tauri-drag-region className="flex h-8 justify-end gap-1">
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
            title={t('app.close')}
            className={button}
            onClick={() => void appWindow.close()}
          >
            <X size={15} />
          </button>
        </header>
        <div className="flex flex-1 items-center justify-center overflow-auto">
          <LoginScreen />
        </div>
      </main>
    </div>
  );
}

export function App() {
  const session = useSession();

  useEffect(() => {
    getUiSettings()
      .then((ui) => applyTheme(ui.theme))
      .catch(() => {});
  }, []);

  if (session?.state === 'signedOut') return <SignedOut />;
  if (session?.state === 'signedIn') return <Shell user={session.user} />;
  return <div className="h-full bg-frame" />;
}
