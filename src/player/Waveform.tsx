import { invoke } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';
import { seek, type Track } from './api';
import { livePosition, usePositionSeconds } from './usePlayer';

const BARS = 48;
const BAR_WIDTH = 2;

/** Flat bars while the real waveform loads or if it can't be fetched. */
const FLAT = Array.from({ length: BARS }, (_, i) => 0.35 + 0.25 * Math.abs(Math.sin(i * 0.7)));

/**
 * The mini-player's waveform strip and progress. Drawn on a canvas from
 * `requestAnimationFrame`; the position never goes through React state.
 */
export function Waveform({ track, playing }: { track: Track | null; playing: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [bars, setBars] = useState<number[]>(FLAT);
  // Keeps tick events flowing and repaints once a second while paused.
  const second = usePositionSeconds();

  useEffect(() => {
    setBars(FLAT);
    if (!track?.waveformUrl) return;
    let active = true;
    invoke<number[]>('player_waveform', { url: track.waveformUrl, count: BARS })
      .then((loaded) => {
        if (active && loaded.length > 0) setBars(loaded);
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
    const played = style.getPropertyValue('--wave-played');
    const rest = style.getPropertyValue('--wave');
    const duration = (track?.durationMs ?? 0) / 1000;

    const draw = () => {
      const ratio = window.devicePixelRatio || 1;
      const width = element.clientWidth;
      const height = element.clientHeight;
      if (element.width !== Math.round(width * ratio)) element.width = Math.round(width * ratio);
      if (element.height !== Math.round(height * ratio))
        element.height = Math.round(height * ratio);
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      const progress = duration > 0 ? Math.min(1, livePosition(playing) / duration) : 0;
      const step = width / bars.length;
      bars.forEach((value, i) => {
        const barHeight = Math.max(3, value * (height - 8));
        context.fillStyle = track && (i + 0.5) / bars.length <= progress ? played : rest;
        context.beginPath();
        context.roundRect(
          i * step + (step - BAR_WIDTH) / 2,
          (height - barHeight) / 2,
          BAR_WIDTH,
          barHeight,
          1,
        );
        context.fill();
      });
    };

    draw();
    if (!playing) return;
    let frame = requestAnimationFrame(function loop() {
      draw();
      frame = requestAnimationFrame(loop);
    });
    return () => cancelAnimationFrame(frame);
  }, [bars, playing, track, second]);

  const seekTo = (event: React.MouseEvent<HTMLCanvasElement>) => {
    if (!track) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const fraction = (event.clientX - rect.left) / rect.width;
    void seek(Math.max(0, fraction) * (track.durationMs / 1000));
  };

  return (
    <canvas
      ref={canvas}
      onClick={seekTo}
      className={`block h-10 w-full ${track ? 'cursor-pointer' : ''}`}
    />
  );
}
