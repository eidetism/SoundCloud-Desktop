import { Pause, Play } from 'lucide-react';
import { useTranslation } from 'react-i18next';

/** The small round play button from the mockup (`solid`: accent; else white on artwork). */
export function PlayCircle({
  playing,
  solid = true,
  size = 26,
  disabled,
  onClick,
}: {
  playing: boolean;
  solid?: boolean;
  size?: number;
  disabled?: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  const icon = Math.round(size * 0.38);
  return (
    <button
      type="button"
      disabled={disabled}
      title={playing ? t('player.pause') : t('player.play')}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      style={{ width: size, height: size }}
      className={`grid flex-none place-items-center rounded-full transition-transform hover:scale-105 disabled:invisible ${
        solid ? 'bg-accent text-on-accent' : 'bg-on-accent text-accent'
      }`}
    >
      {playing ? (
        <Pause size={icon} fill="currentColor" />
      ) : (
        <Play size={icon} fill="currentColor" className="translate-x-px" />
      )}
    </button>
  );
}
