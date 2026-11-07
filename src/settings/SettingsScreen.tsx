import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NetworkSettingsForm } from '../net/NetworkSettingsForm';
import { OfflineSettings } from '../offline/OfflineSettings';
import { applyTheme, getUiSettings, setUiSettings, type Theme, type UiSettings } from './api';

const THEMES: Theme[] = ['system', 'light', 'dark'];

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-center justify-between gap-6 py-3">
      <span>
        <span className="block text-sm font-semibold">{label}</span>
        {hint && <span className="mt-0.5 block text-xs text-muted">{hint}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative h-6 w-11 flex-none rounded-full transition-colors ${checked ? 'bg-accent' : 'bg-line'}`}
      >
        <span
          className={`absolute top-1 h-4 w-4 rounded-full bg-bg transition-[left] ${checked ? 'left-6' : 'left-1'}`}
        />
      </button>
    </label>
  );
}

export function SettingsScreen() {
  const { t } = useTranslation();
  const [ui, setUi] = useState<UiSettings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getUiSettings()
      .then(setUi)
      .catch((reason) => setError(String(reason)));
  }, []);

  const save = (next: UiSettings) => {
    setUi(next);
    applyTheme(next.theme);
    setUiSettings(next).catch((reason) => setError(String(reason)));
  };

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-8 pb-8">
      <h1 className="text-2xl font-bold">{t('nav.settings')}</h1>
      {error && <p className="text-sm text-danger">{error}</p>}
      {ui && (
        <section className="flex flex-col divide-y divide-line">
          <div className="flex items-center justify-between gap-6 py-3">
            <span className="text-sm font-semibold">{t('settings.theme')}</span>
            <div className="flex rounded-full bg-card p-1">
              {THEMES.map((theme) => (
                <button
                  key={theme}
                  type="button"
                  onClick={() => save({ ...ui, theme })}
                  className={`rounded-full px-4 py-1.5 text-xs font-semibold ${
                    ui.theme === theme ? 'bg-accent text-on-accent' : 'text-muted hover:text-text'
                  }`}
                >
                  {t(`settings.themes.${theme}`)}
                </button>
              ))}
            </div>
          </div>
          <Toggle
            label={t('settings.normalize')}
            hint={t('settings.normalizeHint')}
            checked={ui.normalizeVolume}
            onChange={(normalizeVolume) => save({ ...ui, normalizeVolume })}
          />
          <Toggle
            label={t('settings.discord')}
            hint={t('settings.discordHint')}
            checked={ui.discordRichPresence}
            onChange={(discordRichPresence) => save({ ...ui, discordRichPresence })}
          />
        </section>
      )}
      <OfflineSettings />
      <section>
        <NetworkSettingsForm />
      </section>
    </div>
  );
}
