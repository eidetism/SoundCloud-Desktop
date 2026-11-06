import { getCurrentWindow } from '@tauri-apps/api/window';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { logout, type Session } from './auth/api';
import { LoginScreen } from './auth/LoginScreen';
import { useSession } from './auth/useSession';
import { LikesList } from './library/LikesList';
import { NetworkSettingsForm } from './net/NetworkSettingsForm';
import { PlayerBar } from './player/PlayerBar';
import { usePlayer } from './player/usePlayer';

export function App() {
  const { t } = useTranslation();
  const appWindow = getCurrentWindow();
  const session = useSession();

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
      {session?.state === 'signedOut' && (
        <main className="flex flex-1 flex-col items-center justify-center gap-6 overflow-auto rounded-3xl bg-[var(--surface)] p-6">
          <LoginScreen />
        </main>
      )}
      {session?.state === 'signedIn' && <SignedIn session={session} />}
    </div>
  );
}

/** Phase 4 test screen: likes and the player. The real shell comes in phase 5. */
function SignedIn({ session }: { session: Extract<Session, { state: 'signedIn' }> }) {
  const { t } = useTranslation();
  const player = usePlayer();
  const [showNetwork, setShowNetwork] = useState(false);

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-4 rounded-3xl bg-[var(--surface)] p-6">
      <div className="flex items-center gap-3">
        <h1 className="flex-1 text-2xl font-semibold">
          {t('auth.hello', { name: session.user?.username ?? '…' })}
        </h1>
        {!session.verified && (
          <p className="text-sm text-[var(--text-muted)]">{t('auth.unverified')}</p>
        )}
        <button
          type="button"
          onClick={() => setShowNetwork((shown) => !shown)}
          className="h-9 rounded-full border border-[var(--border)] px-5"
        >
          {t('net.title')}
        </button>
        <button
          type="button"
          onClick={() => void logout()}
          className="h-9 rounded-full border border-[var(--border)] px-5"
        >
          {t('auth.signOut')}
        </button>
      </div>
      {showNetwork ? (
        <div className="overflow-auto">
          <NetworkSettingsForm />
        </div>
      ) : (
        <LikesList currentId={player?.track?.id ?? null} />
      )}
      {player && <PlayerBar player={player} />}
    </main>
  );
}
