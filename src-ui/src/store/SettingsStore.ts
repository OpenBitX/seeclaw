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
  permanentlyAllowed: string[] = [];

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

  /** Sync store from a raw backend AppConfig payload (emitted after save). */
  syncFromBackend(raw: Record<string, unknown>): void {
    const llm = (raw.llm as Record<string, unknown>) ?? {};
    const safety = (raw.safety as Record<string, unknown>) ?? {};
    const activeProvider = (llm.active_provider as string) ?? this.settings.activeProvider;
    const rawProviders = (llm.providers as Record<string, unknown>) ?? {};

    const providers: ProviderConfig[] = Object.entries(rawProviders).map(([id, val]) => {
      const p = (val as Record<string, unknown>) ?? {};
      return {
        id,
        displayName: (p.display_name as string) ?? id,
        apiBase: (p.api_base as string) ?? '',
        model: (p.model as string) ?? '',
        temperature: (p.temperature as number) ?? 0.1,
        hasApiKey: !!(p.api_key as string),
      };
    });

    this.settings = {
      ...this.settings,
      activeProvider,
      providers,
      safety: {
        allowTerminalCommands: (safety.allow_terminal_commands as boolean) ?? false,
        allowFileOperations: (safety.allow_file_operations as boolean) ?? false,
        requireApprovalFor: (safety.require_approval_for as string[]) ?? [],
        maxConsecutiveFailures: (safety.max_consecutive_failures as number) ?? 5,
        maxLoopDurationMinutes: (safety.max_loop_duration_minutes as number) ?? 0,
      },
    };
  }

  addPermanentlyAllowed(actionType: string): void {
    if (!this.permanentlyAllowed.includes(actionType)) {
      this.permanentlyAllowed.push(actionType);
    }
  }

  removePermanentlyAllowed(actionType: string): void {
    this.permanentlyAllowed = this.permanentlyAllowed.filter((t) => t !== actionType);
  }
}

export const settingsStore = new SettingsStore();
