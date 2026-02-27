import React, { useState, useEffect, useCallback } from 'react';
import ReactDOM from 'react-dom';
import { observer } from 'mobx-react-lite';
import { invoke } from '@tauri-apps/api/core';
import { motion, AnimatePresence } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import FormControl from '@mui/joy/FormControl';
import FormLabel from '@mui/joy/FormLabel';
import Input from '@mui/joy/Input';
import Switch from '@mui/joy/Switch';
import Checkbox from '@mui/joy/Checkbox';
import RadioGroup from '@mui/joy/RadioGroup';
import Radio from '@mui/joy/Radio';
import Button from '@mui/joy/Button';
import Chip from '@mui/joy/Chip';
import Divider from '@mui/joy/Divider';
import AccordionGroup from '@mui/joy/AccordionGroup';
import Accordion from '@mui/joy/Accordion';
import AccordionSummary from '@mui/joy/AccordionSummary';
import AccordionDetails from '@mui/joy/AccordionDetails';
import Sheet from '@mui/joy/Sheet';
import Stack from '@mui/joy/Stack';
import Select from '@mui/joy/Select';
import Option from '@mui/joy/Option';
import { X, ChevronDown, KeyRound, AlertCircle } from 'lucide-react';
import { settingsStore } from '../../store/SettingsStore';
import presets from '../../data/provider-presets.json';

// ── Types ─────────────────────────────────────────────────────────────────────

interface LocalProviderConfig {
  id: string;
  displayName: string;
  apiKey?: string;
  apiBase: string;
  model: string;
  temperature: number;
  hasApiKey: boolean;
}

interface RoleConfig {
  provider: string;
  model: string;
  stream: boolean;
  temperature?: number;
}

interface McpServer {
  name: string;
  command: string;
  args: string[];
  enabled: boolean;
}

interface LocalSafety {
  allowTerminalCommands: boolean;
  allowFileOperations: boolean;
  requireApprovalFor: string[];
  maxConsecutiveFailures: number;
  maxLoopDurationMinutes: number;
}

interface LocalConfig {
  activeProvider: string;
  providers: LocalProviderConfig[];
  roles: {
    routing?: RoleConfig;
    chat?: RoleConfig;
    tools?: RoleConfig;
    vision?: RoleConfig;
  };
  safety: LocalSafety;
  theme: 'light' | 'dark' | 'system';
  mcpServers: McpServer[];
}

type PresetsMap = Record<string, { displayName: string; models: string[] }>;
const PRESETS: PresetsMap = presets as PresetsMap;

// ── Constants ─────────────────────────────────────────────────────────────────

const BUILTIN_TOOLS: string[] = [
  'mouse_click', 'mouse_double_click', 'mouse_right_click', 'scroll',
  'type_text', 'hotkey', 'key_press', 'get_viewport',
  'execute_terminal', 'mcp_call', 'invoke_skill', 'wait',
  'finish_task', 'report_failure',
];

const ROLES: Array<{ key: keyof LocalConfig['roles']; label: string }> = [
  { key: 'routing', label: 'routing' },
  { key: 'chat', label: 'chat' },
  { key: 'tools', label: 'tools' },
  { key: 'vision', label: 'vision' },
];

const DEFAULT_CONFIG: LocalConfig = {
  activeProvider: 'zhipu',
  providers: [],
  roles: {},
  safety: {
    allowTerminalCommands: false,
    allowFileOperations: false,
    requireApprovalFor: ['execute_terminal', 'mcp_call'],
    maxConsecutiveFailures: 5,
    maxLoopDurationMinutes: 0,
  },
  theme: 'system',
  mcpServers: [],
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/** A provider is "configured" when it has a key set (either in config or env). */
function isProviderConfigured(p: LocalProviderConfig) {
  return p.hasApiKey || (p.apiKey && p.apiKey.trim() !== '' && p.apiKey !== '***');
}

// ── Config mappings ───────────────────────────────────────────────────────────

function mapBackendConfig(raw: Record<string, unknown>): LocalConfig {
  const llm = (raw.llm as Record<string, unknown>) ?? {};
  const safety = (raw.safety as Record<string, unknown>) ?? {};
  const mcp = (raw.mcp as Record<string, unknown>) ?? {};
  const activeProvider = (llm.active_provider as string) ?? 'zhipu';
  const rawProviders = (llm.providers as Record<string, unknown>) ?? {};

  const providers: LocalProviderConfig[] = Object.entries(rawProviders).map(([id, val]) => {
    const p = (val as Record<string, unknown>) ?? {};
    return {
      id,
      displayName: (p.display_name as string) ?? id,
      apiKey: (p.api_key as string) ?? '',
      apiBase: (p.api_base as string) ?? '',
      model: (p.model as string) ?? '',
      temperature: (p.temperature as number) ?? 0.1,
      hasApiKey: Boolean(p.api_key),
    };
  });

  const rawRoles = (llm.roles as Record<string, unknown>) ?? {};
  const roles: LocalConfig['roles'] = {};
  for (const key of ['routing', 'chat', 'tools', 'vision'] as const) {
    const r = rawRoles[key] as Record<string, unknown> | undefined;
    if (r) {
      roles[key] = {
        provider: (r.provider as string) ?? activeProvider,
        model: (r.model as string) ?? '',
        stream: (r.stream as boolean) ?? false,
        temperature: r.temperature as number | undefined,
      };
    }
  }

  const rawServers = (mcp.servers as Array<Record<string, unknown>>) ?? [];
  return {
    activeProvider,
    providers,
    roles,
    safety: {
      allowTerminalCommands: (safety.allow_terminal_commands as boolean) ?? false,
      allowFileOperations: (safety.allow_file_operations as boolean) ?? false,
      requireApprovalFor: (safety.require_approval_for as string[]) ?? [],
      maxConsecutiveFailures: (safety.max_consecutive_failures as number) ?? 5,
      maxLoopDurationMinutes: (safety.max_loop_duration_minutes as number) ?? 0,
    },
    theme: (raw.theme as 'light' | 'dark' | 'system') ?? 'system',
    mcpServers: rawServers.map((s) => ({
      name: (s.name as string) ?? '',
      command: (s.command as string) ?? '',
      args: (s.args as string[]) ?? [],
      enabled: (s.enabled as boolean) ?? false,
    })),
  };
}

function mapLocalToBackend(local: LocalConfig): Record<string, unknown> {
  const providers: Record<string, unknown> = {};
  for (const p of local.providers) {
    providers[p.id] = {
      display_name: p.displayName,
      api_base: p.apiBase,
      model: p.model,
      temperature: p.temperature,
      api_key: p.apiKey ?? null,
      adapter: null,
    };
  }
  const roles: Record<string, unknown> = {};
  for (const [key, role] of Object.entries(local.roles)) {
    if (role) {
      roles[key] = {
        provider: role.provider,
        model: role.model,
        stream: role.stream,
        temperature: role.temperature ?? null,
      };
    }
  }
  return {
    llm: { active_provider: local.activeProvider, providers, roles },
    safety: {
      allow_terminal_commands: local.safety.allowTerminalCommands,
      allow_file_operations: local.safety.allowFileOperations,
      require_approval_for: local.safety.requireApprovalFor,
      max_consecutive_failures: local.safety.maxConsecutiveFailures,
      max_loop_duration_minutes: local.safety.maxLoopDurationMinutes,
    },
    prompts: { tools_file: '', system_template: '', experience_summary_template: '' },
    mcp: {
      servers: local.mcpServers.map((s) => ({
        name: s.name, command: s.command, args: s.args, enabled: s.enabled,
      })),
    },
  };
}

// ── Tab button ────────────────────────────────────────────────────────────────

function TabBtn({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <Box
      component="button"
      onClick={onClick}
      sx={{
        position: 'relative',
        px: 1.5,
        py: 1,
        fontSize: 'sm',
        fontWeight: active ? 600 : 400,
        color: active ? 'text.primary' : 'text.secondary',
        background: 'none',
        border: 'none',
        cursor: 'pointer',
        borderRadius: 'sm',
        outline: 'none',
        transition: 'color 0.15s',
        '&:hover': { color: 'text.primary' },
        '&::after': active
          ? {
              content: '""',
              position: 'absolute',
              bottom: 0, left: '50%',
              transform: 'translateX(-50%)',
              width: '60%', height: '2px',
              bgcolor: 'text.primary',
              borderRadius: '1px',
            }
          : {},
      }}
    >
      {label}
    </Box>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

const SettingsModal: React.FC = observer(() => {
  const [activeTab, setActiveTab] = useState(0);
  const [config, setConfig] = useState<LocalConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [toast, setToast] = useState<{ msg: string; ok: boolean } | null>(null);

  const isOpen = settingsStore.isSettingsOpen;

  useEffect(() => {
    if (!isOpen) return;
    setActiveTab(0);
    setLoading(true);
    setLoadError(null);
    invoke<Record<string, unknown>>('get_config')
      .then((raw) => setConfig(mapBackendConfig(raw)))
      .catch((err) => setLoadError(String(err)))
      .finally(() => setLoading(false));
  }, [isOpen]);

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(() => setToast(null), 2800);
    return () => clearTimeout(id);
  }, [toast]);

  const close = useCallback(() => settingsStore.closeSettings(), []);

  const saveConfig = useCallback(async () => {
    setSaving(true);
    try {
      await invoke('save_config_ui', { payload: mapLocalToBackend(config) });
      settingsStore.setTheme(config.theme);
      setToast({ msg: '设置已保存', ok: true });
      settingsStore.closeSettings();
    } catch (err) {
      setToast({ msg: `保存失败：${String(err)}`, ok: false });
    } finally {
      setSaving(false);
    }
  }, [config]);

  const updateProvider = (id: string, field: keyof LocalProviderConfig, value: unknown) =>
    setConfig((prev) => ({
      ...prev,
      providers: prev.providers.map((p) => (p.id === id ? { ...p, [field]: value } : p)),
    }));

  const updateRole = (role: keyof LocalConfig['roles'], field: keyof RoleConfig, value: unknown) =>
    setConfig((prev) => ({
      ...prev,
      roles: {
        ...prev.roles,
        [role]: {
          provider: prev.activeProvider, model: '', stream: false,
          ...(prev.roles[role] ?? {}), [field]: value,
        },
      },
    }));

  const updateSafety = (field: keyof LocalSafety, value: unknown) =>
    setConfig((prev) => ({ ...prev, safety: { ...prev.safety, [field]: value } }));

  const toggleApproval = (tool: string) => {
    const cur = config.safety.requireApprovalFor;
    updateSafety('requireApprovalFor', cur.includes(tool) ? cur.filter((t) => t !== tool) : [...cur, tool]);
  };

  const updateMcpServer = (idx: number, field: keyof McpServer, value: unknown) =>
    setConfig((prev) => ({
      ...prev,
      mcpServers: prev.mcpServers.map((s, i) => (i === idx ? { ...s, [field]: value } : s)),
    }));

  // ── Fixed dialog dimensions ───────────────────────────────────────────────
  // Height is ALWAYS fixed — content fades in after load, no layout jump ever.

  const modal = (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18 }}
            onClick={close}
            style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.36)', zIndex: 1200 }}
          />

          <div style={{
            position: 'fixed', inset: 0,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            zIndex: 1201, pointerEvents: 'none',
          }}>
            <motion.div
              key="dialog"
              initial={{ opacity: 0, scale: 0.96, y: 6 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.96, y: 6 }}
              transition={{ type: 'spring', stiffness: 480, damping: 38 }}
              style={{
                pointerEvents: 'auto',
                // Fixed dimensions — never changes, no layout jump
                width: 'min(580px, calc(100vw - 32px))',
                height: 'min(600px, calc(100vh - 64px))',
                display: 'flex',
                flexDirection: 'column',
                borderRadius: 12,
                border: '1px solid var(--joy-palette-neutral-outlinedBorder)',
                background: 'var(--joy-palette-background-popup)',
                boxShadow: '0 4px 6px -1px rgba(0,0,0,0.08), 0 16px 48px -8px rgba(0,0,0,0.20)',
              }}
            >
              {/* Header */}
              <Box sx={{
                px: 3, py: 2,
                borderBottom: '1px solid', borderColor: 'divider',
                display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                flexShrink: 0,
                bgcolor: 'background.surface',
                borderTopLeftRadius: 11, borderTopRightRadius: 11,
              }}>
                <Typography level="title-md" fontWeight={600}>设置</Typography>
                <Box component="button" onClick={close} sx={{
                  display: 'flex', alignItems: 'center', justifyContent: 'center',
                  width: 28, height: 28, borderRadius: 'sm',
                  border: 'none', background: 'none', cursor: 'pointer', outline: 'none',
                  color: 'text.secondary',
                  '&:hover': { bgcolor: 'neutral.100', color: 'text.primary' },
                }}>
                  <X size={15} />
                </Box>
              </Box>

              {/* Tab bar */}
              <Box sx={{
                display: 'flex', alignItems: 'center', gap: 0.5,
                px: 3, pt: 1, pb: 0,
                borderBottom: '1px solid', borderColor: 'divider',
                flexShrink: 0,
                bgcolor: 'background.surface',
              }}>
                <TabBtn label="基础设置" active={activeTab === 0} onClick={() => setActiveTab(0)} />
                <TabBtn label="高级设置" active={activeTab === 1} onClick={() => setActiveTab(1)} />
              </Box>

              {/* Scrollable content — FIXED flex:1, content fades in, no height jump */}
              <Box sx={{ flex: 1, overflowY: 'auto', overflowX: 'hidden', position: 'relative' }}>
                {loadError ? (
                  <Box sx={{ p: 4, display: 'flex', alignItems: 'center', gap: 1 }}>
                    <AlertCircle size={16} color="var(--joy-palette-danger-500)" />
                    <Typography level="body-sm" color="danger">加载失败：{loadError}</Typography>
                  </Box>
                ) : (
                  // Always render content; fade it in when ready. Zero layout jump.
                  <motion.div
                    animate={{ opacity: loading ? 0 : 1 }}
                    transition={{ duration: 0.2, ease: 'easeOut' }}
                    // Keep pointer-events off while loading to prevent interaction
                    style={{ pointerEvents: loading ? 'none' : 'auto' }}
                  >
                    <AnimatePresence mode="wait" initial={false}>
                      <motion.div
                        key={activeTab}
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={{ duration: 0.14 }}
                      >
                        {activeTab === 0 ? (
                          <BasicTab
                            config={config}
                            setConfig={setConfig}
                            updateProvider={updateProvider}
                            updateRole={updateRole}
                          />
                        ) : (
                          <AdvancedTab
                            config={config}
                            updateSafety={updateSafety}
                            toggleApproval={toggleApproval}
                            updateMcpServer={updateMcpServer}
                          />
                        )}
                      </motion.div>
                    </AnimatePresence>
                  </motion.div>
                )}
              </Box>

              {/* Footer */}
              <Box sx={{
                px: 3, py: 2,
                borderTop: '1px solid', borderColor: 'divider',
                display: 'flex', justifyContent: 'flex-end', gap: 1,
                flexShrink: 0,
                bgcolor: 'background.surface',
                borderBottomLeftRadius: 11, borderBottomRightRadius: 11,
              }}>
                <Button variant="outlined" color="neutral" size="sm" onClick={close} disabled={saving}>
                  取消
                </Button>
                <Button variant="solid" color="neutral" size="sm" onClick={saveConfig} loading={saving}>
                  保存
                </Button>
              </Box>
            </motion.div>
          </div>
        </>
      )}
    </AnimatePresence>
  );

  const toastEl = (
    <AnimatePresence>
      {toast && (
        <motion.div
          key="toast"
          initial={{ opacity: 0, y: 12, scale: 0.96 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 8, scale: 0.96 }}
          transition={{ type: 'spring', stiffness: 500, damping: 35 }}
          style={{
            position: 'fixed', bottom: 24, right: 24, zIndex: 9998,
            background: toast.ok ? 'var(--joy-palette-success-500)' : 'var(--joy-palette-danger-500)',
            color: '#fff', borderRadius: 8, padding: '10px 18px',
            fontSize: 13, fontWeight: 500,
            boxShadow: '0 4px 16px rgba(0,0,0,0.18)',
          }}
        >
          {toast.msg}
        </motion.div>
      )}
    </AnimatePresence>
  );

  return ReactDOM.createPortal(<>{modal}{toastEl}</>, document.body);
});

// ── BasicTab ──────────────────────────────────────────────────────────────────

function BasicTab({
  config, setConfig, updateProvider, updateRole,
}: {
  config: LocalConfig;
  setConfig: React.Dispatch<React.SetStateAction<LocalConfig>>;
  updateProvider: (id: string, field: keyof LocalProviderConfig, value: unknown) => void;
  updateRole: (role: keyof LocalConfig['roles'], field: keyof RoleConfig, value: unknown) => void;
}) {
  const configuredProviders = config.providers.filter(isProviderConfigured);
  const hasAnyProvider = config.providers.length > 0;
  const hasAnyConfigured = configuredProviders.length > 0;

  return (
    <Stack spacing={0} divider={<Divider />}>
      {/* Section A: Provider */}
      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 1.5 }}>
          模型提供商
        </Typography>

        {!hasAnyProvider ? (
          // No providers in config at all
          <Sheet variant="soft" color="neutral" sx={{ p: 2.5, borderRadius: 'md', textAlign: 'center' }}>
            <Box sx={{ color: 'text.tertiary', mb: 0.5 }}>
              <KeyRound size={20} />
            </Box>
            <Typography level="body-sm" color="neutral">
              未找到模型提供商配置
            </Typography>
            <Typography level="body-xs" color="neutral" sx={{ mt: 0.5 }}>
              请在项目根目录创建 <code>config.toml</code> 并配置提供商
            </Typography>
          </Sheet>
        ) : (
          <RadioGroup
            value={config.activeProvider}
            onChange={(e) => setConfig((prev) => ({ ...prev, activeProvider: e.target.value }))}
          >
            <AccordionGroup variant="outlined" sx={{ borderRadius: 'md' }}>
              {config.providers.map((provider) => {
                const preset = PRESETS[provider.id];
                const presetModels: string[] = preset?.models ?? [];
                const listId = `models-${provider.id}`;
                const configured = isProviderConfigured(provider);
                return (
                  <Accordion key={provider.id}>
                    <AccordionSummary indicator={<ChevronDown size={14} />}>
                      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, width: '100%', pr: 0.5 }}>
                        <Radio value={provider.id} size="sm" onClick={(e) => e.stopPropagation()} />
                        <Typography level="body-sm" fontWeight="md">
                          {provider.displayName}
                        </Typography>
                        {config.activeProvider === provider.id && (
                          <Chip size="sm" color="neutral" variant="soft" sx={{ ml: 'auto', mr: 1 }}>
                            当前
                          </Chip>
                        )}
                        {!configured && (
                          <Chip
                            size="sm"
                            color="warning"
                            variant="soft"
                            sx={{ ml: configured ? 0 : 'auto', mr: 1 }}
                          >
                            未配置
                          </Chip>
                        )}
                      </Box>
                    </AccordionSummary>
                    <AccordionDetails>
                      <Stack spacing={1.5} sx={{ pt: 0.5, pb: 1 }}>
                        <FormControl>
                          <FormLabel>API Key</FormLabel>
                          <Input
                            type="password"
                            size="sm"
                            value={provider.apiKey ?? ''}
                            onChange={(e) => updateProvider(provider.id, 'apiKey', e.target.value)}
                            placeholder={
                              provider.hasApiKey
                                ? '已通过配置文件设置（留空保持不变）'
                                : '输入 API Key 或留空使用环境变量'
                            }
                          />
                        </FormControl>
                        <FormControl>
                          <FormLabel>Base URL</FormLabel>
                          <Input
                            type="url"
                            size="sm"
                            value={provider.apiBase}
                            onChange={(e) => updateProvider(provider.id, 'apiBase', e.target.value)}
                          />
                        </FormControl>
                        <FormControl>
                          <FormLabel>默认模型</FormLabel>
                          {presetModels.length > 0 && (
                            <datalist id={listId}>
                              {presetModels.map((m) => <option key={m} value={m} />)}
                            </datalist>
                          )}
                          <Input
                            size="sm"
                            value={provider.model}
                            onChange={(e) => updateProvider(provider.id, 'model', e.target.value)}
                            placeholder={presetModels[0] ?? '模型名称'}
                            slotProps={presetModels.length > 0 ? { input: { list: listId } } : undefined}
                          />
                        </FormControl>
                        <FormControl>
                          <FormLabel>Temperature</FormLabel>
                          <Input
                            type="number"
                            size="sm"
                            value={provider.temperature}
                            onChange={(e) =>
                              updateProvider(provider.id, 'temperature', parseFloat(e.target.value))
                            }
                            slotProps={{ input: { min: 0, max: 2, step: 0.1 } }}
                            sx={{ maxWidth: 120 }}
                          />
                        </FormControl>
                      </Stack>
                    </AccordionDetails>
                  </Accordion>
                );
              })}
            </AccordionGroup>
          </RadioGroup>
        )}
      </Box>

      {/* Section B: Role config */}
      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 1.5 }}>
          角色模型配置
        </Typography>

        {!hasAnyConfigured ? (
          // No configured providers yet — guide the user
          <Sheet
            variant="soft"
            color="warning"
            sx={{ p: 2, borderRadius: 'md', display: 'flex', gap: 1.5, alignItems: 'flex-start' }}
          >
            <AlertCircle size={16} style={{ marginTop: 2, flexShrink: 0 }} />
            <Box>
              <Typography level="body-sm" fontWeight="md">
                请先配置至少一个提供商
              </Typography>
              <Typography level="body-xs" sx={{ mt: 0.25 }}>
                在上方「模型提供商」中填写 API Key 并保存后，可在此为每个角色指定模型。
              </Typography>
            </Box>
          </Sheet>
        ) : (
          <Stack spacing={1}>
            {ROLES.map(({ key, label }) => {
              const role = config.roles[key] ?? {
                provider: config.activeProvider,
                model: '',
                stream: false,
              };
              // Only show configured providers in the dropdown
              const rolePresetModels = PRESETS[role.provider]?.models ?? [];
              const roleListId = `role-models-${key}`;
              return (
                <Box key={key} sx={{ display: 'flex', alignItems: 'center', gap: 1.5 }}>
                  <Chip
                    size="sm"
                    variant="outlined"
                    color="neutral"
                    sx={{ minWidth: 68, justifyContent: 'center', fontFamily: 'code' }}
                  >
                    {label}
                  </Chip>
                  <Select
                    size="sm"
                    value={configuredProviders.some((p) => p.id === role.provider)
                      ? role.provider
                      : (configuredProviders[0]?.id ?? role.provider)
                    }
                    onChange={(_, val) => val && updateRole(key, 'provider', val)}
                    sx={{ flex: 1, minWidth: 0 }}
                  >
                    {/* Only show providers that have an API key set */}
                    {configuredProviders.map((p) => (
                      <Option key={p.id} value={p.id}>
                        {p.displayName}
                      </Option>
                    ))}
                  </Select>
                  {rolePresetModels.length > 0 && (
                    <datalist id={roleListId}>
                      {rolePresetModels.map((m) => <option key={m} value={m} />)}
                    </datalist>
                  )}
                  <Input
                    size="sm"
                    value={role.model}
                    onChange={(e) => updateRole(key, 'model', e.target.value)}
                    placeholder={rolePresetModels[0] ?? '模型名称'}
                    slotProps={rolePresetModels.length > 0 ? { input: { list: roleListId } } : undefined}
                    sx={{ flex: 2, minWidth: 0 }}
                  />
                </Box>
              );
            })}
          </Stack>
        )}
      </Box>

      {/* Section C: Theme */}
      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 1.5 }}>界面主题</Typography>
        <RadioGroup
          orientation="horizontal"
          value={config.theme}
          onChange={(e) => {
            const t = e.target.value as 'light' | 'dark' | 'system';
            setConfig((prev) => ({ ...prev, theme: t }));
            settingsStore.setTheme(t);
          }}
          sx={{ gap: 2 }}
        >
          <Radio value="light" label="浅色" size="sm" />
          <Radio value="dark" label="深色" size="sm" />
          <Radio value="system" label="跟随系统" size="sm" />
        </RadioGroup>
      </Box>
    </Stack>
  );
}

// ── AdvancedTab ───────────────────────────────────────────────────────────────

function AdvancedTab({
  config, updateSafety, toggleApproval, updateMcpServer,
}: {
  config: LocalConfig;
  updateSafety: (field: keyof LocalSafety, value: unknown) => void;
  toggleApproval: (tool: string) => void;
  updateMcpServer: (idx: number, field: keyof McpServer, value: unknown) => void;
}) {
  return (
    <Stack spacing={0} divider={<Divider />}>
      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 1.5 }}>安全与权限</Typography>
        <Stack spacing={1.5}>
          <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Typography level="body-sm">允许终端命令（无需确认）</Typography>
            <Switch size="sm" checked={config.safety.allowTerminalCommands}
              onChange={(e) => updateSafety('allowTerminalCommands', e.target.checked)} />
          </Box>
          <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Typography level="body-sm">允许文件操作（无需确认）</Typography>
            <Switch size="sm" checked={config.safety.allowFileOperations}
              onChange={(e) => updateSafety('allowFileOperations', e.target.checked)} />
          </Box>
          <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Typography level="body-sm">最大连续失败次数</Typography>
            <Input type="number" size="sm" value={config.safety.maxConsecutiveFailures}
              onChange={(e) => updateSafety('maxConsecutiveFailures', parseInt(e.target.value, 10) || 1)}
              slotProps={{ input: { min: 1, max: 20 } }} sx={{ width: 80 }} />
          </Box>
          <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <Typography level="body-sm">最大运行时间（分钟，0=无限）</Typography>
            <Input type="number" size="sm" value={config.safety.maxLoopDurationMinutes}
              onChange={(e) => updateSafety('maxLoopDurationMinutes', parseInt(e.target.value, 10) || 0)}
              slotProps={{ input: { min: 0 } }} sx={{ width: 80 }} />
          </Box>
        </Stack>
      </Box>

      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 0.5 }}>高危操作审批列表</Typography>
        <Typography level="body-xs" color="neutral" sx={{ mb: 1.5 }}>
          选中的操作在执行前需要用户手动确认
        </Typography>
        <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 0.75 }}>
          {BUILTIN_TOOLS.map((tool) => (
            <Checkbox key={tool} size="sm"
              checked={config.safety.requireApprovalFor.includes(tool)}
              onChange={() => toggleApproval(tool)} label={tool} sx={{ fontFamily: 'code' }} />
          ))}
        </Box>
      </Box>

      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 0.5 }}>永久允许列表</Typography>
        <Typography level="body-xs" color="neutral" sx={{ mb: 1.5 }}>
          这些操作类型将跳过审批弹窗，直接执行
        </Typography>
        {settingsStore.permanentlyAllowed.length > 0 ? (
          <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 0.75 }}>
            {settingsStore.permanentlyAllowed.map((item) => (
              <Chip key={item} size="sm" variant="soft" color="warning">{item}</Chip>
            ))}
          </Box>
        ) : (
          <Typography level="body-xs" color="neutral">（列表为空）</Typography>
        )}
      </Box>

      <Box sx={{ p: 3 }}>
        <Typography level="title-sm" sx={{ mb: 1.5 }}>MCP 服务器</Typography>
        {config.mcpServers.length === 0 ? (
          <Typography level="body-xs" color="neutral">暂无 MCP 服务器配置</Typography>
        ) : (
          <Stack spacing={1}>
            {config.mcpServers.map((server, idx) => (
              <Sheet key={idx} variant="outlined" sx={{
                p: 1.5, borderRadius: 'sm',
                display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 2,
              }}>
                <Box sx={{ minWidth: 0, flex: 1 }}>
                  <Typography level="body-sm" fontWeight="md">{server.name}</Typography>
                  <Typography level="body-xs" color="neutral" sx={{
                    fontFamily: 'code', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {server.command} {server.args.join(' ')}
                  </Typography>
                </Box>
                <Switch size="sm" checked={server.enabled}
                  onChange={(e) => updateMcpServer(idx, 'enabled', e.target.checked)} />
              </Sheet>
            ))}
          </Stack>
        )}
      </Box>
    </Stack>
  );
}

export { SettingsModal };
export default SettingsModal;
