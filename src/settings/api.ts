import { invoke } from '@tauri-apps/api/core';

export type Theme = 'system' | 'light' | 'dark';

export interface UiSettings {
  theme: Theme;
  discordRichPresence: boolean;
  normalizeVolume: boolean;
}

export const getUiSettings = () => invoke<UiSettings>('settings_ui_get');
export const setUiSettings = (ui: UiSettings) => invoke<UiSettings>('settings_ui_set', { ui });

/** `system` follows Windows via `prefers-color-scheme`; the others pin it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = theme;
}
