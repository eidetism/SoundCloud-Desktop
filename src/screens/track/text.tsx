import { invoke } from '@tauri-apps/api/core';

/** Open a web link in the system browser (Rust accepts only http(s)). */
export const openLink = (url: string) => void invoke('app_open_link', { url });

export const profileUrl = (permalink: string) => `https://soundcloud.com/${permalink}`;

const TOKEN = /(https?:\/\/[^\s<>"]+)|(^|[\s(])@([\w-]{2,})/g;
const TRAILING = /[.,;:!?)\]]+$/;

/**
 * Plain text with clickable links and `@mentions` (both open in the browser:
 * a permalink can't be resolved to a profile without an endpoint we have).
 */
export function RichText({ text }: { text: string }) {
  const parts: React.ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(TOKEN)) {
    const start = match.index ?? 0;
    if (match[1]) {
      const url = match[1].replace(TRAILING, '');
      parts.push(text.slice(last, start));
      parts.push(
        <button
          key={start}
          type="button"
          onClick={() => openLink(url)}
          className="text-accent [overflow-wrap:anywhere] hover:underline"
        >
          {url}
        </button>,
      );
      last = start + url.length;
    } else {
      const lead = match[2] ?? '';
      const permalink = match[3];
      parts.push(text.slice(last, start + lead.length));
      parts.push(
        <button
          key={start}
          type="button"
          onClick={() => openLink(profileUrl(permalink))}
          className="text-accent hover:underline"
        >
          @{permalink}
        </button>,
      );
      last = start + match[0].length;
    }
  }
  parts.push(text.slice(last));
  return <>{parts}</>;
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
];

/** "2 ч назад" / "2 hours ago". */
export function timeAgo(iso: string, language: string): string {
  const seconds = (Date.now() - new Date(iso).getTime()) / 1000;
  const format = new Intl.RelativeTimeFormat(language, { numeric: 'auto', style: 'short' });
  for (const [unit, size] of UNITS) {
    if (seconds >= size) return format.format(-Math.floor(seconds / size), unit);
  }
  return format.format(0, 'minute');
}
