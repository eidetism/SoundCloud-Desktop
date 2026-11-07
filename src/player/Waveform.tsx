import { invoke } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';
import { seek, type Track } from './api';
import { formatTime, livePosition, notePosition, usePositionSeconds } from './usePlayer';

/** Peaks fetched once per track; drawing groups them to fit the width. */
const SOURCE_BARS = 400;
const BAR_GAP = 1;

/** Shown while the real waveform loads or if it can't be fetched. */
const FLAT = Array.from({ length: 120 }, (_, i) => 0.3 + 0.2 * Math.abs(Math.sin(i * 0.7)));

/** Group `source` peaks into `count` bars, keeping each group's peak. */
function fit(source: number[], count: number): number[] {
  return Array.from({ length: count }, (_, i) => {
    const start = Math.floor((i * source.length) / count);
    const end = Math.max(start + 1, Math.floor(((i + 1) * source.length) / count));
    let peak = 0;
    for (let j = start; j < end && j < source.length; j++) peak = Math.max(peak, source[j]);
    return peak;
  });
}

const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

const seekCurrent = (seconds: number) => {
  notePosition(seconds);
  void seek(seconds);
};

/**
 * A seek strip: the track's real waveform on a canvas, played part in the
 * accent. Click or drag to seek, hover shows the time. Drawn from
 * `requestAnimationFrame`; the position never goes through React state.
 * `active`: the track is the player's current one (otherwise no progress is
 * drawn and `onSeek` decides what a click does).
 */
export function Waveform({
  track,
  playing,
  active = true,
  onSeek = seekCurrent,
  barWidth = 2,
  restColor = '--bar-wave',
  className = 'h-6 min-w-0 flex-1',
}: {
  track: Track | null;
  playing: boolean;
  active?: boolean;
  onSeek?: (seconds: number) => void;
  barWidth?: number;
  /** CSS variable for the unplayed bars. */
  restColor?: string;
  className?: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const hoverTip = useRef<HTMLDivElement>(null);
  const drag = useRef<number | null>(null);
  const redraw = useRef<() => void>(() => {});
  const [peaks, setPeaks] = useState<number[]>(FLAT);
  // Keeps tick events flowing and repaints once a second while paused.
  const second = usePositionSeconds();
  const duration = (track?.durationMs ?? 0) / 1000;

  useEffect(() => {
    setPeaks(FLAT);
    if (!track?.waveformUrl) return;
    let active = true;
    invoke<number[]>('player_waveform', { url: track.waveformUrl, count: SOURCE_BARS })
      .then((loaded) => {
        if (active && loaded.length > 0) setPeaks(loaded);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [track?.waveformUrl]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `second` repaints a paused strip after a seek.
  useEffect(() => {
    const element = canvas.current;
    const context = element?.getContext('2d');
    if (!element || !context) return;
    const style = getComputedStyle(element);
    const played = style.getPropertyValue('--accent');
    const rest = style.getPropertyValue(restColor);
    let cached = { width: 0, bars: [] as number[] };

    const draw = () => {
      const ratio = window.devicePixelRatio || 1;
      const width = element.clientWidth;
      const height = element.clientHeight;
      if (width === 0) return;
      if (element.width !== Math.round(width * ratio)) element.width = Math.round(width * ratio);
      if (element.height !== Math.round(height * ratio))
        element.height = Math.round(height * ratio);
      if (cached.width !== width) {
        const count = Math.max(1, Math.floor((width + BAR_GAP) / (barWidth + BAR_GAP)));
        cached = { width, bars: fit(peaks, count) };
      }
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      const progress =
        drag.current ?? (active && duration > 0 ? clamp01(livePosition(playing) / duration) : 0);
      const { bars } = cached;
      const step = width / bars.length;
      bars.forEach((value, i) => {
        const barHeight = Math.max(2, value * height);
        context.fillStyle = track && (i + 0.5) / bars.length <= progress ? played : rest;
        context.fillRect(i * step, (height - barHeight) / 2, barWidth, barHeight);
      });
    };
    redraw.current = draw;

    draw();
    const resize = new ResizeObserver(draw);
    resize.observe(element);
    let frame = 0;
    if (playing && active) {
      frame = requestAnimationFrame(function loop() {
        draw();
        frame = requestAnimationFrame(loop);
      });
    }
    return () => {
      cancelAnimationFrame(frame);
      resize.disconnect();
    };
  }, [peaks, playing, active, track, duration, second, barWidth, restColor]);

  const fractionAt = (event: React.PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return clamp01((event.clientX - rect.left) / rect.width);
  };

  const showHover = (event: React.PointerEvent<HTMLElement>) => {
    const tip = hoverTip.current;
    if (!tip || !track) return;
    const fraction = fractionAt(event);
    tip.textContent = formatTime(fraction * duration);
    tip.style.left = `${fraction * 100}%`;
    tip.style.opacity = '1';
  };

  return (
    <div
      className={`relative ${className} ${track ? 'cursor-pointer' : ''}`}
      onPointerDown={(event) => {
        if (!track || event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = fractionAt(event);
        redraw.current();
      }}
      onPointerMove={(event) => {
        showHover(event);
        if (drag.current === null) return;
        drag.current = fractionAt(event);
        redraw.current();
      }}
      onPointerUp={(event) => {
        if (drag.current === null) return;
        const target = fractionAt(event) * duration;
        drag.current = null;
        onSeek(target);
        redraw.current();
      }}
      onPointerCancel={() => {
        drag.current = null;
        redraw.current();
      }}
      onPointerLeave={() => {
        if (hoverTip.current) hoverTip.current.style.opacity = '0';
      }}
    >
      <canvas ref={canvas} className="block h-full w-full" />
      <div
        ref={hoverTip}
        className="pointer-events-none absolute -top-7 -translate-x-1/2 rounded-md bg-side-2 px-1.5 py-0.5 text-[11px] text-side-hi tabular-nums opacity-0 transition-opacity"
      />
    </div>
  );
}
