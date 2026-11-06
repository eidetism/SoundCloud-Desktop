import type { LucideIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';

/** A section whose screen isn't built yet. */
export function Placeholder({ title, icon: Icon }: { title: string; icon: LucideIcon }) {
  const { t } = useTranslation();
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
      <div className="grid h-16 w-16 place-items-center rounded-lg bg-card text-muted">
        <Icon size={28} />
      </div>
      <h1 className="text-xl font-bold">{title}</h1>
      <p className="text-sm text-muted">{t('shell.soon')}</p>
    </div>
  );
}
