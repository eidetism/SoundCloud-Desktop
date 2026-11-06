import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NetworkSettingsForm } from '../net/NetworkSettingsForm';
import { startLogin } from './api';

export function LoginScreen() {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);
  const [showNetwork, setShowNetwork] = useState(false);

  const login = () => {
    setError(null);
    startLogin().catch((reason) => setError(String(reason)));
  };

  return (
    <div className="flex flex-col items-center gap-4 text-center">
      <h1 className="text-2xl font-semibold">{t('auth.title')}</h1>
      <p className="max-w-sm text-sm text-[var(--muted)]">{t('auth.explain')}</p>
      <button
        type="button"
        onClick={login}
        className="h-11 rounded-full bg-[var(--accent)] px-8 font-semibold text-white"
      >
        {t('auth.signIn')}
      </button>
      {error && <p className="max-w-sm text-sm text-[var(--danger)]">{error}</p>}
      <button
        type="button"
        onClick={() => setShowNetwork((shown) => !shown)}
        className="text-sm text-[var(--muted)] underline"
      >
        {t('auth.networkSettings')}
      </button>
      {showNetwork && <NetworkSettingsForm />}
    </div>
  );
}
