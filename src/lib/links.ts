import { invoke } from '@tauri-apps/api/core';

/** Open a web link in the system browser (Rust accepts only http(s)). */
export const openLink = (url: string) => void invoke('app_open_link', { url });
