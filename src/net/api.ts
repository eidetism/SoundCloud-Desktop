import { invoke } from '@tauri-apps/api/core';

export type NetMode = 'direct' | 'proxy';
export type ProxyScheme = 'socks5' | 'http' | 'https';

export interface ProxyEndpoint {
  scheme: ProxyScheme;
  host: string;
  port: number;
  username: string;
}

export interface NetworkSettings {
  mode: NetMode;
  proxy: ProxyEndpoint;
}

export interface NetworkSettingsView {
  settings: NetworkSettings;
  hasPassword: boolean;
}

export type PasswordChange =
  | { action: 'keep' }
  | { action: 'set'; value: string }
  | { action: 'clear' };

export interface CheckResult {
  host: string;
  status: number | null;
  latencyMs: number | null;
  error: { kind: 'timeout' | 'connect' | 'other'; message: string } | null;
}

export const getNetworkSettings = () => invoke<NetworkSettingsView>('net_settings_get');

export const setNetworkSettings = (settings: NetworkSettings, password: PasswordChange) =>
  invoke<NetworkSettingsView>('net_settings_set', { settings, password });

export const checkConnection = (draft: NetworkSettings, password: PasswordChange) =>
  invoke<CheckResult[]>('net_check', { draft, password });
