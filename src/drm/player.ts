/**
 * The DRM player page (hidden window `drm`, see `src-tauri/src/player/drm.rs`).
 *
 * Plays one CENC track at a time with Media Source Extensions and Widevine
 * (EME). It never touches the network: encrypted segments come from Rust over
 * IPC, and every CDM message is handed to Rust, which POSTs it to SoundCloud's
 * licence server. Decryption stays inside the CDM.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Command =
  | { kind: 'load'; generation: number; durations: number[]; volume: number; start: number }
  | { kind: 'play' }
  | { kind: 'pause' }
  | { kind: 'seek'; seconds: number }
  | { kind: 'volume'; level: number }
  | { kind: 'stop' };

const MIME = 'audio/mp4; codecs="mp4a.40.2"';
const KEY_SYSTEM = 'com.widevine.alpha';
/** Seconds kept buffered ahead of and behind the playhead. */
const AHEAD = 60;
const BEHIND = 20;
const REPORT_EVERY_MS = 200;
/** Bumped with `PAGE_VERSION` in `player/drm.rs`; tells a stale cached page apart. */
const PAGE_VERSION = 2;
/** A Widevine request this slow means a broken module; Rust then resets the profile. */
const KEY_SYSTEM_TIMEOUT_MS = 15_000;
/** Rust matches this message (`player/drm.rs`). */
const KEY_SYSTEM_STUCK = 'key system request timed out';

let access: Promise<MediaKeySystemAccess> | null = null;

function keySystem(): Promise<MediaKeySystemAccess> {
  access ??= navigator.requestMediaKeySystemAccess(KEY_SYSTEM, [
    {
      initDataTypes: ['cenc'],
      audioCapabilities: [{ contentType: MIME, robustness: 'SW_SECURE_CRYPTO' }],
      distinctiveIdentifier: 'optional',
      persistentState: 'optional',
      sessionTypes: ['temporary'],
    },
  ]);
  return access;
}

const describe = (error: unknown) => (error instanceof Error ? error.message : String(error));
/** `HTMLMediaElement.volume` throws outside [0, 1]; never let it kill the track. */
const volumeLevel = (value: number) => Math.min(1, Math.max(0, Number.isFinite(value) ? value : 1));

/** A diagnostic line for the log, independent of any track (boot, errors). */
const diag = (message: string) => void invoke('drm_diag', { message }).catch(() => {});

/** One track: an `<audio>` element fed segment by segment. */
class Track {
  private readonly audio = new Audio();
  private readonly source = new MediaSource();
  private readonly starts: number[];
  private readonly total: number;
  private buffer: SourceBuffer | null = null;
  private keySession: MediaKeySession | null = null;
  private url: string;
  private next = 0;
  private pumping = false;
  private disposed = false;
  private lastReport = 0;

  constructor(
    readonly generation: number,
    private readonly durations: number[],
    volume: number,
    private readonly startAt: number,
  ) {
    let start = 0;
    this.starts = durations.map((duration) => {
      const at = start;
      start += duration;
      return at;
    });
    this.total = start;
    this.audio.volume = volumeLevel(volume);
    this.url = URL.createObjectURL(this.source);
  }

  async start() {
    this.stage('requesting key system');
    const system = await Promise.race([
      keySystem(),
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(KEY_SYSTEM_STUCK)), KEY_SYSTEM_TIMEOUT_MS),
      ),
    ]);
    this.stage(`key system ${system.keySystem}`);
    const keys = await system.createMediaKeys();
    this.stage('media keys created');
    await this.audio.setMediaKeys(keys);
    this.stage('media keys attached');
    this.audio.addEventListener('playing', () => this.stage('playing'), { once: true });
    this.audio.addEventListener('encrypted', (event) => void this.onEncrypted(keys, event));
    this.audio.addEventListener('timeupdate', () => this.onTime());
    this.audio.addEventListener('seeking', () => this.onSeeking());
    this.audio.addEventListener('ended', () => this.report('ended'));
    this.audio.addEventListener('error', () =>
      this.fail(this.audio.error?.message || `media error ${this.audio.error?.code}`),
    );

    const opened = new Promise((resolve) =>
      this.source.addEventListener('sourceopen', resolve, { once: true }),
    );
    this.audio.src = this.url;
    await opened;
    if (this.disposed) return;
    this.buffer = this.source.addSourceBuffer(MIME);
    this.source.duration = this.total;
    await this.append(await this.segment(-1));
    this.stage('init segment appended');
    if (this.startAt > 0) {
      this.next = this.segmentAt(this.startAt);
      this.audio.currentTime = Math.min(this.startAt, this.total);
    }
    await this.pump();
    await this.audio.play();
  }

  play() {
    this.audio.play().catch((error) => this.fail(describe(error)));
  }

  pause() {
    this.audio.pause();
  }

  seek(seconds: number) {
    this.audio.currentTime = Math.min(Math.max(0, seconds), this.total);
  }

  setVolume(level: number) {
    this.audio.volume = volumeLevel(level);
  }

  dispose() {
    this.disposed = true;
    this.audio.pause();
    this.audio.removeAttribute('src');
    this.audio.load();
    URL.revokeObjectURL(this.url);
    void this.keySession?.close().catch(() => {});
  }

  private async onEncrypted(keys: MediaKeys, event: MediaEncryptedEvent) {
    if (this.keySession || !event.initData) return;
    this.stage(`encrypted (${event.initDataType})`);
    const session = keys.createSession('temporary');
    this.keySession = session;
    session.addEventListener('message', (message) => void this.license(session, message));
    try {
      await session.generateRequest(event.initDataType, event.initData);
      this.stage('licence request generated');
    } catch (error) {
      this.fail(`licence request: ${describe(error)}`);
    }
  }

  /** A CDM message (certificate or licence request) goes through Rust. */
  private async license(session: MediaKeySession, message: MediaKeyMessageEvent) {
    try {
      const answer = await invoke<ArrayBuffer>('drm_license', new Uint8Array(message.message), {
        headers: { 'x-generation': String(this.generation) },
      });
      if (!this.disposed) await session.update(answer);
    } catch (error) {
      this.fail(`licence: ${describe(error)}`);
    }
  }

  private onTime() {
    const now = performance.now();
    if (now - this.lastReport >= REPORT_EVERY_MS) {
      this.lastReport = now;
      this.report('position');
    }
    void this.pump();
  }

  /** Jumping outside what is buffered restarts feeding from that segment. */
  private onSeeking() {
    this.report('position');
    const time = this.audio.currentTime;
    if (!this.isBuffered(time)) {
      this.next = this.segmentAt(time);
      void this.dropAll().then(() => this.pump());
    } else {
      void this.pump();
    }
  }

  /** Keep about `AHEAD` seconds buffered, dropping what is far behind. */
  private async pump() {
    if (this.pumping || !this.buffer) return;
    this.pumping = true;
    try {
      while (!this.disposed && this.next < this.durations.length) {
        const time = this.audio.currentTime;
        if (this.bufferedEnd(time) - time >= AHEAD) break;
        await this.dropBehind(time);
        const index = this.next;
        const bytes = await this.segment(index);
        if (this.disposed) return;
        if (index !== this.next) continue;
        await this.append(bytes);
        this.next = index + 1;
      }
      if (
        !this.disposed &&
        this.next >= this.durations.length &&
        this.source.readyState === 'open'
      ) {
        this.source.endOfStream();
      }
    } catch (error) {
      this.fail(describe(error));
    } finally {
      this.pumping = false;
    }
  }

  private segment(index: number): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>('drm_segment', { generation: this.generation, index });
  }

  private segmentAt(time: number): number {
    let index = 0;
    while (index + 1 < this.starts.length && this.starts[index + 1] <= time + 0.05) index++;
    return index;
  }

  private isBuffered(time: number): boolean {
    const ranges = this.audio.buffered;
    for (let i = 0; i < ranges.length; i++) {
      if (ranges.start(i) <= time + 0.1 && time < ranges.end(i)) return true;
    }
    return false;
  }

  private bufferedEnd(time: number): number {
    const ranges = this.audio.buffered;
    for (let i = 0; i < ranges.length; i++) {
      if (ranges.start(i) <= time + 0.1 && time < ranges.end(i)) return ranges.end(i);
    }
    return time;
  }

  private async dropBehind(time: number) {
    const ranges = this.audio.buffered;
    if (ranges.length > 0 && time - BEHIND > ranges.start(0) + 1) {
      await this.update((buffer) => buffer.remove(0, time - BEHIND));
    }
  }

  private async dropAll() {
    if (this.audio.buffered.length > 0) {
      await this.update((buffer) => buffer.remove(0, this.total + 1));
    }
  }

  private append(bytes: ArrayBuffer) {
    return this.update((buffer) => buffer.appendBuffer(bytes));
  }

  /** Run one SourceBuffer operation and wait for it to finish. */
  private async update(operation: (buffer: SourceBuffer) => void) {
    const buffer = this.buffer;
    if (!buffer || this.disposed) return;
    while (buffer.updating) {
      await new Promise((resolve) => buffer.addEventListener('updateend', resolve, { once: true }));
    }
    await new Promise<void>((resolve, reject) => {
      const done = () => {
        buffer.removeEventListener('error', failed);
        resolve();
      };
      const failed = () => {
        buffer.removeEventListener('updateend', done);
        reject(new Error('SourceBuffer error'));
      };
      buffer.addEventListener('updateend', done, { once: true });
      buffer.addEventListener('error', failed, { once: true });
      operation(buffer);
    });
  }

  /** A start-up step, for the log (where a stuck start stopped). */
  private stage(message: string) {
    this.report('stage', message);
  }

  report(event: 'position' | 'ended' | 'error' | 'stage', message?: string) {
    if (this.disposed && event !== 'error') return;
    void invoke('drm_report', {
      generation: this.generation,
      event,
      position: this.audio.currentTime,
      message: message ?? null,
    }).catch(() => {});
  }

  fail(message: string) {
    if (this.disposed) return;
    this.report('error', message);
    this.dispose();
  }
}

let current: Track | null = null;

function handle(command: Command) {
  switch (command.kind) {
    case 'load': {
      try {
        current?.dispose();
        const track = new Track(
          command.generation,
          command.durations,
          command.volume,
          command.start,
        );
        current = track;
        track.start().catch((error) => track.fail(describe(error)));
      } catch (error) {
        diag(`load failed: ${describe(error)}`);
      }
      break;
    }
    case 'play':
      current?.play();
      break;
    case 'pause':
      current?.pause();
      break;
    case 'seek':
      current?.seek(command.seconds);
      break;
    case 'volume':
      current?.setVolume(command.level);
      break;
    case 'stop':
      current?.dispose();
      current = null;
      break;
  }
}

/** Listen first, then tell Rust we're up; it answers with a waiting track. */
export async function start() {
  window.addEventListener('error', (event) =>
    diag(`window error: ${event.message} (${event.filename}:${event.lineno})`),
  );
  window.addEventListener('unhandledrejection', (event) =>
    diag(`unhandled rejection: ${describe(event.reason)}`),
  );
  const caps = [
    `MediaSource=${typeof MediaSource !== 'undefined'}`,
    `EME=${typeof navigator.requestMediaKeySystemAccess === 'function'}`,
    `MSE(${MIME})=${typeof MediaSource !== 'undefined' && MediaSource.isTypeSupported(MIME)}`,
    `secure=${window.isSecureContext}`,
  ].join(' ');
  diag(`capabilities: ${caps}`);
  try {
    await listen<Command>('drm:command', (event) => handle(event.payload));
    const pending = await invoke<Command | null>('drm_boot', { page: PAGE_VERSION });
    if (pending) handle(pending);
  } catch (error) {
    diag(`boot failed: ${describe(error)}`);
  }
}
