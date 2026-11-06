import { type ImageSize, scimg } from '../lib/scimg';

/** Artwork or avatar through `scimg://`, with a neutral tile when there is none. */
export function Cover({
  url,
  size,
  className,
  blank = 'bg-card',
}: {
  url: string | null | undefined;
  size: ImageSize;
  className: string;
  /** Background of the empty tile and of the image while it loads. */
  blank?: string;
}) {
  if (!url) return <div className={`${blank} ${className}`} />;
  return (
    <img
      src={scimg(url, size)}
      alt=""
      loading="lazy"
      decoding="async"
      draggable={false}
      className={`${blank} object-cover ${className}`}
    />
  );
}
