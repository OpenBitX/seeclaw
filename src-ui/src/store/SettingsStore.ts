import { makeAutoObservable } from 'mobx';
import type { AppSettings, ProviderConfig } from '../types/settings';

const DEFAULT_SETTINGS: AppSettings = {
  activeProvider: 'zhipu',
  providers: [],
  safety: {
    allowTerminalCommands: false,
    allowFileOperations: false,
    requireApprovalFor: ['execute_terminal', 'mcp_call'],
    maxConsecutiveFailures: 5,
    maxLoopDurationMinutes: 0,
  },
  loopDefaults: {
    mode: 'until_done',
    maxDurationMinutes: 60,
    maxFailures: 5,
  },
  theme: 'system',
  userLanguage: 'auto',
};

class SettingsStore {
  settings: AppSettings = DEFAULT_SETTINGS;
  isLoading = false;
  isSettingsOpen = false;

  constructor() {
    makeAutoObservable(this);
  }

  get activeProvider(): ProviderConfig | undefined {
    return this.settings.providers.find((p) => p.id === this.settings.activeProvider);
  }

  setSettings(settings: AppSettings): void {
    this.settings = settings;
  }

  setLoading(loading: boolean): void {
    this.isLoading = loading;
  }

  openSettings(): void {
    this.isSettingsOpen = true;
  }

  closeSettings(): void {
    this.isSettingsOpen = false;
  }

  setTheme(theme: 'light' | 'dark' | 'system'): void {
    this.settings = { ...this.settings, theme };
  }
}

export const settingsStore = new SettingsStore();
