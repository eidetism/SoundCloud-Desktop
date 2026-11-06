import { useId } from 'react';

/** Pulse bars from `assets/brand/1-pulse.svg` (viewBox 1024): [x, y, height]. */
const BARS: [number, number, number][] = [
  [140, 452, 120],
  [226, 402, 220],
  [312, 332, 360],
  [398, 252, 520],
  [484, 192, 640],
  [570, 252, 520],
  [656, 332, 360],
  [742, 402, 220],
  [828, 452, 120],
];

/** Corner radius of the sidebar tile: 14px at 52px, kept proportional. */
const RADIUS = (14 / 52) * 1024;

export function AppLogo({ size }: { size: number }) {
  const gradient = useId();
  return (
    <svg
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      className="pointer-events-none block"
      aria-hidden
    >
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" style={{ stopColor: 'var(--g1)' }} />
          <stop offset="1" style={{ stopColor: 'var(--g2)' }} />
        </linearGradient>
      </defs>
      <rect width="1024" height="1024" rx={RADIUS} fill={`url(#${gradient})`} />
      {BARS.map(([x, y, h]) => (
        <rect key={x} x={x} y={y} width="56" height={h} rx="28" className="fill-on-accent" />
      ))}
    </svg>
  );
}
