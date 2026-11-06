import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface Me {
  id: number;
  username: string;
  permalink: string;
  avatarUrl: string | null;
  fullName: string | null;
}

export type Session =
  | { state: 'signedOut' }
  | { state: 'signedIn'; user: Me | null; verified: boolean };

export const getSession = () => invoke<Session>('auth_status');
export const startLogin = () => invoke<void>('auth_login_start');
export const logout = () => invoke<void>('auth_logout');

export const onSessionChange = (handler: (session: Session) => void) =>
  listen<Session>('auth:changed', (event) => handler(event.payload));
