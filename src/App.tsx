import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTranslation } from 'react-i18next';
import { NetworkSettingsForm } from './net/NetworkSettingsForm';

export function App() {
  const { t } = useTranslation();
  const appWindow = getCurrentWindow();

  return (
    <div className="flex h-full flex-col p-3">
      <header data-tauri-drag-region className="flex h-8 items-center justify-end gap-1 px-2">
        <button
          type="button"
          aria-label={t('app.minimize')}
          onClick={() => void appWindow.minimize()}
          className="h-6 w-6 rounded text-[var(--text-muted)] hover:bg-white/10"
        >
          –
        </button>
        <button
          type="button"
          aria-label={t('app.close')}
          onClick={() => void appWindow.close()}
          className="h-6 w-6 rounded text-[var(--text-muted)] hover:bg-[var(--accent)] hover:text-white"
        >
          ×
        </button>
      </header>
      <main className="flex flex-1 flex-col items-center justify-center gap-6 rounded-3xl bg-[var(--surface)]">
        <h1 className="text-2xl font-semibold">{t('app.name')}</h1>
        <p className="text-[var(--text-muted)]">{t('app.skeleton')}</p>
        <NetworkSettingsForm />
      </main>
    </div>
  );
}
