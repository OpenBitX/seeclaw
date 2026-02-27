export interface ProviderConfig {
  id: string;
  displayName: string;
  apiBase: string;
  model: string;
  temperature: number;
  hasApiKey: boolean;
}

export interface SafetyConfig {
  allowTerminalCommands: boolean;
  allowFileOperations: boolean;
  requireApprovalFor: string[];
  maxConsecutiveFailures: number;
  maxLoopDurationMinutes: number;
}

export interface LoopDefaults {
  mode: 'until_done' | 'timed' | 'failure_limit';
  maxDurationMinutes: number;
  maxFailures: number;
}

export interface AppSettings {
  activeProvider: string;
  providers: ProviderConfig[];
  safety: SafetyConfig;
  loopDefaults: LoopDefaults;
  theme: 'light' | 'dark' | 'system';
  userLanguage: 'auto' | 'zh' | 'en';
}
