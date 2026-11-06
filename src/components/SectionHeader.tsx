import { useTranslation } from 'react-i18next';

/** "Ваши миксы ··· Все" from the mockup. */
export function SectionHeader({ title, onAll }: { title: string; onAll?: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="mt-5 mb-3 flex items-baseline justify-between">
      <b className="text-[15px]">{title}</b>
      {onAll && (
        <button type="button" onClick={onAll} className="text-[11px] text-muted hover:text-text">
          {t('common.all')}
        </button>
      )}
    </div>
  );
}
