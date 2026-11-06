import { useEffect } from 'react';
import { togglePlay } from './api';

/** Where Space types or picks something and must keep its normal meaning. */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  return (
    target instanceof HTMLInputElement && !['range', 'checkbox', 'radio'].includes(target.type)
  );
}

/** Space plays/pauses anywhere in the window, except while typing. */
export function useSpaceToggle() {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.code !== 'Space' || event.ctrlKey || event.altKey || event.metaKey) return;
      if (isTyping(event.target)) return;
      // Also keeps a focused button from being "clicked" by the same key.
      event.preventDefault();
      if (!event.repeat) void togglePlay();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
