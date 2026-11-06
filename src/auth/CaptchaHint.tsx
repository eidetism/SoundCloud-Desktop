import { useTranslation } from 'react-i18next';

/** Content of the small window pinned over the login window during the DataDome check. */
export function CaptchaHint() {
  const { t } = useTranslation();
  return (
    <div className="flex h-full items-center justify-center bg-[var(--accent)] px-4 text-center text-sm font-semibold text-white">
      {t('auth.captchaHint')}
    </div>
  );
}
