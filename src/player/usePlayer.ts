import { useEffect, useState, useSyncExternalStore } from 'react';
import { getPlayerState, onPlayerState, onPosition, type PlayerSnapshot } from './api';

/** The Rust-owned player state; `null` until the first answer. Has no position. */
export function usePlayer(): PlayerSnapshot | null {
  const [snapshot, setSnapshot] = useState<PlayerSnapshot | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = onPlayerState((next) => {
      if (active) setSnapshot(next);
    });
    getPlayerState().then((current) => {
      if (active) setSnapshot((previous) => previous ?? current);
    });
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return snapshot;
}

// Position lives outside React state: one listener, and components that
// subscribe re-render only when the whole second changes.
let seconds = 0;
let lastTick = { value: 0, at: 0 };
const subscribers = new Set<() => void>();
let unlisten: Promise<() => void> | null = null;

function subscribe(onChange: () => void) {
  subscribers.add(onChange);
  unlisten ??= onPosition((value) => {
    lastTick = { value, at: performance.now() };
    const floored = Math.floor(value);
    if (floored !== seconds) {
      seconds = floored;
      for (const notify of subscribers) notify();
    }
  });
  return () => {
    subscribers.delete(onChange);
    if (subscribers.size === 0 && unlisten) {
      void unlisten.then((stop) => stop());
      unlisten = null;
    }
  };
}

/** Whole seconds played. Subscribe only from leaf components. */
export function usePositionSeconds(): number {
  return useSyncExternalStore(subscribe, () => seconds);
}

/**
 * Position in seconds, interpolated between ticks while playing. For
 * `requestAnimationFrame` drawing only; components must not render from it.
 * Needs a `usePositionSeconds` subscriber mounted to receive ticks.
 */
export function livePosition(playing: boolean): number {
  if (!playing || lastTick.at === 0) return lastTick.value;
  return lastTick.value + Math.min(1, (performance.now() - lastTick.at) / 1000);
}

/**
 * Pretend a tick for `value` arrived now, so a seek shows at once instead of
 * jumping back until Rust's next tick.
 */
export function notePosition(value: number) {
  lastTick = { value, at: performance.now() };
}

export function formatTime(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(s / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const seconds = String(s % 60).padStart(2, '0');
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${seconds}`
    : `${minutes}:${seconds}`;
}
