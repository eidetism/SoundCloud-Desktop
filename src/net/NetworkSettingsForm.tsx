import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  type CheckResult,
  checkConnection,
  getNetworkSettings,
  type NetworkSettings,
  type PasswordChange,
  type ProxyScheme,
  setNetworkSettings,
} from './api';

const SCHEMES: ProxyScheme[] = ['socks5', 'http', 'https'];

const field =
  'h-9 rounded-lg border border-[var(--line)] bg-transparent px-3 outline-none focus:border-[var(--accent)]';

export function NetworkSettingsForm() {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<NetworkSettings | null>(null);
  const [hasPassword, setHasPassword] = useState(false);
  const [password, setPassword] = useState('');
  const [clearPassword, setClearPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [results, setResults] = useState<CheckResult[]>([]);

  useEffect(() => {
    getNetworkSettings()
      .then((view) => {
        setDraft(view.settings);
        setHasPassword(view.hasPassword);
      })
      .catch((error) => setMessage(String(error)));
  }, []);

  if (!draft) return message ? <p className="text-[var(--danger)]">{message}</p> : null;

  const passwordChange = (): PasswordChange => {
    if (password) return { action: 'set', value: password };
    if (clearPassword) return { action: 'clear' };
    return { action: 'keep' };
  };

  const updateProxy = (patch: Partial<NetworkSettings['proxy']>) =>
    setDraft({ ...draft, proxy: { ...draft.proxy, ...patch } });

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setMessage(null);
    try {
      await action();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  };

  const save = () =>
    run(async () => {
      const view = await setNetworkSettings(draft, passwordChange());
      setDraft(view.settings);
      setHasPassword(view.hasPassword);
      setPassword('');
      setClearPassword(false);
      setMessage(t('net.saved'));
    });

  const check = () =>
    run(async () => {
      setResults(await checkConnection(draft, passwordChange()));
    });

  const proxy = draft.mode === 'proxy';

  return (
    <section className="flex w-[440px] flex-col gap-3 text-sm">
      <h2 className="text-base font-semibold">{t('net.title')}</h2>

      <div className="flex gap-4">
        {(['direct', 'proxy'] as const).map((mode) => (
          <label key={mode} className="flex items-center gap-2">
            <input
              type="radio"
              checked={draft.mode === mode}
              onChange={() => setDraft({ ...draft, mode })}
            />
            {t(`net.mode.${mode}`)}
          </label>
        ))}
      </div>

      <fieldset disabled={!proxy} className="flex flex-col gap-2 disabled:opacity-50">
        <div className="flex gap-2">
          <select
            className={field}
            value={draft.proxy.scheme}
            onChange={(e) => updateProxy({ scheme: e.target.value as ProxyScheme })}
          >
            {SCHEMES.map((scheme) => (
              <option key={scheme} value={scheme}>
                {t(`net.scheme.${scheme}`)}
              </option>
            ))}
          </select>
          <input
            className={`${field} min-w-0 flex-1`}
            placeholder={t('net.host')}
            value={draft.proxy.host}
            onChange={(e) => updateProxy({ host: e.target.value })}
          />
          <input
            className={`${field} w-24`}
            placeholder={t('net.port')}
            inputMode="numeric"
            value={draft.proxy.port || ''}
            onChange={(e) => updateProxy({ port: Number(e.target.value.replace(/\D/g, '')) || 0 })}
          />
        </div>
        <div className="flex gap-2">
          <input
            className={`${field} min-w-0 flex-1`}
            placeholder={t('net.username')}
            value={draft.proxy.username}
            onChange={(e) => updateProxy({ username: e.target.value })}
          />
          <input
            className={`${field} min-w-0 flex-1`}
            type="password"
            autoComplete="off"
            placeholder={hasPassword && !clearPassword ? t('net.passwordSaved') : t('net.password')}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
        {hasPassword && (
          <label className="flex items-center gap-2 text-[var(--muted)]">
            <input
              type="checkbox"
              checked={clearPassword}
              onChange={(e) => setClearPassword(e.target.checked)}
            />
            {t('net.clearPassword')}
          </label>
        )}
      </fieldset>

      <div className="flex gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={save}
          className="h-9 rounded-full bg-[var(--accent)] px-5 font-semibold text-white disabled:opacity-50"
        >
          {t('net.save')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={check}
          className="h-9 rounded-full border border-[var(--line)] px-5 disabled:opacity-50"
        >
          {busy ? t('net.checking') : t('net.check')}
        </button>
      </div>

      {message && <p className="text-[var(--muted)]">{message}</p>}

      {results.length > 0 && (
        <ul className="flex flex-col gap-1">
          {results.map((result) => (
            <li key={result.host} className="flex gap-2">
              <span className={result.error ? 'text-[var(--danger)]' : 'text-[var(--success)]'}>
                {result.error ? '✕' : '✓'}
              </span>
              <span className="font-semibold">{result.host}</span>
              <span className="min-w-0 break-words text-[var(--muted)]">
                {result.error
                  ? `${t(`net.error.${result.error.kind}`)}: ${result.error.message}`
                  : t('net.reachable', { ms: result.latencyMs, status: result.status })}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
