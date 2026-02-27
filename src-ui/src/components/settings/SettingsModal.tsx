import React, { useState, useEffect, useCallback } from 'react';
import { observer } from 'mobx-react-lite';
import { invoke } from '@tauri-apps/api/core';
import Modal from '@mui/joy/Modal';
import ModalDialog from '@mui/joy/ModalDialog';
import ModalClose from '@mui/joy/ModalClose';
import Tabs from '@mui/joy/Tabs';
import TabList from '@mui/joy/TabList';
import Tab from '@mui/joy/Tab';
import TabPanel from '@mui/joy/TabPanel';
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
import Snackbar from '@mui/joy/Snackbar';
import { settingsStore } from '../../store/SettingsStore';

// ── Local types ─────────────────────────────────────────────────────────────

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

// ── Constants ────────────────────────────────────────────────────────────────

const BUILTIN_TOOLS: string[] = [
  'mouse_click',
  'mouse_double_click',
  'mouse_right_click',
  'scroll',
  'type_text',
  'hotkey',
  'key_press',
  'get_viewport',
  'execute_terminal',
  'mcp_call',
  'invoke_skill',
  'wait',
  'finish_task',
  'report_failure',
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

// ── Backend config mapping ───────────────────────────────────────────────────

function mapBackendConfig(raw: Record<string, unknown>): LocalConfig {
  const llm = (raw.llm as Record<string, unknown>) ?? {};
  const safety = (raw.safety as Record<string, unknown>) ?? {};
  const mcp = (raw.mcp as Record<string, unknown>) ?? {};

  const activeProvider = (llm.active_provider as string) ?? 'zhipu';
  const rawProviders = (llm.providers as Record<string, unknown>) ?? {};

  const providers: LocalProviderConfig[] = Object.entries(rawProviders).map(
    ([id, val]) => {
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
    },
  );

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

// ── Component ────────────────────────────────────────────────────────────────

const SettingsModal: React.FC = observer(() => {
  const [config, setConfig] = useState<LocalConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [snackbar, setSnackbar] = useState<{
    open: boolean;
    message: string;
    color: 'success' | 'danger';
  }>({ open: false, message: '', color: 'success' });

  // Load config whenever the modal opens
  useEffect(() => {
    if (!settingsStore.isSettingsOpen) return;
    setLoading(true);
    setLoadError(null);
    invoke<Record<string, unknown>>('get_config')
      .then((raw) => setConfig(mapBackendConfig(raw)))
      .catch((err) => setLoadError(String(err)))
      .finally(() => setLoading(false));
  }, [settingsStore.isSettingsOpen]);

  const saveConfig = useCallback(async () => {
    setSaving(true);
    try {
      await invoke('save_config_ui', { payload: config });
      settingsStore.setTheme(config.theme);
      setSnackbar({ open: true, message: '设置已保存', color: 'success' });
    } catch (err) {
      setSnackbar({ open: true, message: `保存失败：${String(err)}`, color: 'danger' });
    } finally {
      setSaving(false);
    }
  }, [config]);

  // Updater helpers
  const updateProvider = (
    id: string,
    field: keyof LocalProviderConfig,
    value: unknown,
  ) => {
    setConfig((prev) => ({
      ...prev,
      providers: prev.providers.map((p) =>
        p.id === id ? { ...p, [field]: value } : p,
      ),
    }));
  };

  const updateRole = (
    role: keyof LocalConfig['roles'],
    field: keyof RoleConfig,
    value: unknown,
  ) => {
    setConfig((prev) => ({
      ...prev,
      roles: {
        ...prev.roles,
        [role]: {
          provider: prev.activeProvider,
          model: '',
          stream: false,
          ...(prev.roles[role] ?? {}),
          [field]: value,
        },
      },
    }));
  };

  const updateSafety = (field: keyof LocalSafety, value: unknown) => {
    setConfig((prev) => ({ ...prev, safety: { ...prev.safety, [field]: value } }));
  };

  const toggleApproval = (tool: string) => {
    const current = config.safety.requireApprovalFor;
    const updated = current.includes(tool)
      ? current.filter((t) => t !== tool)
      : [...current, tool];
    updateSafety('requireApprovalFor', updated);
  };

  const updateMcpServer = (idx: number, field: keyof McpServer, value: unknown) => {
    setConfig((prev) => ({
      ...prev,
      mcpServers: prev.mcpServers.map((s, i) =>
        i === idx ? { ...s, [field]: value } : s,
      ),
    }));
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <>
      <Modal
        open={settingsStore.isSettingsOpen}
        onClose={() => settingsStore.closeSettings()}
      >
        <ModalDialog
          sx={{
            p: 0,
            overflow: 'hidden',
            minWidth: 520,
            maxWidth: 680,
            maxHeight: '90vh',
            display: 'flex',
            flexDirection: 'column',
          }}
        >
          {/* ── Header ── */}
          <Box
            sx={{
              px: 3,
              py: 2,
              borderBottom: '1px solid',
              borderColor: 'divider',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              flexShrink: 0,
            }}
          >
            <Typography level="title-lg">设置</Typography>
            <ModalClose sx={{ position: 'static' }} />
          </Box>

          {/* ── Scrollable content ── */}
          <Box sx={{ overflowY: 'auto', maxHeight: 'calc(90vh - 120px)', flex: 1 }}>
            {loading ? (
              <Box sx={{ p: 4, display: 'flex', justifyContent: 'center' }}>
                <Typography level="body-sm" color="neutral">
                  正在加载配置…
                </Typography>
              </Box>
            ) : loadError ? (
              <Box sx={{ p: 4 }}>
                <Typography level="body-sm" color="danger">
                  加载失败：{loadError}
                </Typography>
              </Box>
            ) : (
              <Tabs defaultValue={0} sx={{ bgcolor: 'transparent' }}>
                <TabList sx={{ px: 3, pt: 1 }}>
                  <Tab value={0}>基础设置</Tab>
                  <Tab value={1}>高级设置</Tab>
                </TabList>

                {/* ════════════════════════════════════
                    Tab 1 — Basic Settings
                ════════════════════════════════════ */}
                <TabPanel value={0} sx={{ p: 3 }}>
                  <Stack spacing={3}>

                    {/* Section A: Provider Configuration */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 1.5 }}>
                        模型提供商
                      </Typography>
                      <RadioGroup
                        value={config.activeProvider}
                        onChange={(e) =>
                          setConfig((prev) => ({
                            ...prev,
                            activeProvider: e.target.value,
                          }))
                        }
                      >
                        <AccordionGroup variant="outlined" sx={{ borderRadius: 'md' }}>
                          {config.providers.map((provider) => (
                            <Accordion key={provider.id}>
                              <AccordionSummary>
                                <Box
                                  sx={{
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 1.5,
                                    width: '100%',
                                    pr: 1,
                                  }}
                                >
                                  <Radio
                                    value={provider.id}
                                    size="sm"
                                    onClick={(e) => e.stopPropagation()}
                                  />
                                  <Typography level="body-sm" fontWeight="md">
                                    {provider.displayName}
                                  </Typography>
                                  {config.activeProvider === provider.id && (
                                    <Chip
                                      size="sm"
                                      color="primary"
                                      variant="soft"
                                      sx={{ ml: 'auto' }}
                                    >
                                      当前
                                    </Chip>
                                  )}
                                </Box>
                              </AccordionSummary>
                              <AccordionDetails>
                                <Stack spacing={1.5} sx={{ pt: 1, pb: 0.5 }}>
                                  <FormControl>
                                    <FormLabel>API Key</FormLabel>
                                    <Input
                                      type="password"
                                      size="sm"
                                      value={provider.apiKey ?? ''}
                                      onChange={(e) =>
                                        updateProvider(provider.id, 'apiKey', e.target.value)
                                      }
                                      placeholder="输入 API Key 或留空使用环境变量"
                                    />
                                  </FormControl>
                                  <FormControl>
                                    <FormLabel>Base URL</FormLabel>
                                    <Input
                                      type="url"
                                      size="sm"
                                      value={provider.apiBase}
                                      onChange={(e) =>
                                        updateProvider(provider.id, 'apiBase', e.target.value)
                                      }
                                    />
                                  </FormControl>
                                  <FormControl>
                                    <FormLabel>默认模型</FormLabel>
                                    <Input
                                      type="text"
                                      size="sm"
                                      value={provider.model}
                                      onChange={(e) =>
                                        updateProvider(provider.id, 'model', e.target.value)
                                      }
                                    />
                                  </FormControl>
                                  <FormControl>
                                    <FormLabel>Temperature</FormLabel>
                                    <Input
                                      type="number"
                                      size="sm"
                                      value={provider.temperature}
                                      onChange={(e) =>
                                        updateProvider(
                                          provider.id,
                                          'temperature',
                                          parseFloat(e.target.value),
                                        )
                                      }
                                      slotProps={{ input: { min: 0, max: 2, step: 0.1 } }}
                                      sx={{ maxWidth: 120 }}
                                    />
                                  </FormControl>
                                </Stack>
                              </AccordionDetails>
                            </Accordion>
                          ))}
                        </AccordionGroup>
                      </RadioGroup>
                    </Box>

                    <Divider />

                    {/* Section B: Role Configuration */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 1.5 }}>
                        角色模型配置
                      </Typography>
                      <Stack spacing={1}>
                        {ROLES.map(({ key, label }) => {
                          const role = config.roles[key] ?? {
                            provider: config.activeProvider,
                            model: '',
                            stream: false,
                          };
                          return (
                            <Box
                              key={key}
                              sx={{ display: 'flex', alignItems: 'center', gap: 1.5 }}
                            >
                              <Chip
                                size="sm"
                                variant="outlined"
                                color="neutral"
                                sx={{ minWidth: 68, justifyContent: 'center' }}
                              >
                                {label}
                              </Chip>
                              <Select
                                size="sm"
                                value={role.provider}
                                onChange={(_, val) =>
                                  val && updateRole(key, 'provider', val)
                                }
                                sx={{ flex: 1, minWidth: 0 }}
                              >
                                {config.providers.map((p) => (
                                  <Option key={p.id} value={p.id}>
                                    {p.displayName}
                                  </Option>
                                ))}
                              </Select>
                              <Input
                                size="sm"
                                value={role.model}
                                onChange={(e) =>
                                  updateRole(key, 'model', e.target.value)
                                }
                                placeholder="模型名称"
                                sx={{ flex: 2, minWidth: 0 }}
                              />
                            </Box>
                          );
                        })}
                      </Stack>
                    </Box>

                    <Divider />

                    {/* Section C: UI Preferences */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 1.5 }}>
                        界面偏好
                      </Typography>
                      <FormControl>
                        <FormLabel>主题</FormLabel>
                        <RadioGroup
                          orientation="horizontal"
                          value={config.theme}
                          onChange={(e) => {
                            const t = e.target.value as 'light' | 'dark' | 'system';
                            setConfig((prev) => ({ ...prev, theme: t }));
                            settingsStore.setTheme(t);
                          }}
                          sx={{ gap: 2, mt: 0.5 }}
                        >
                          <Radio value="light" label="浅色" size="sm" />
                          <Radio value="dark" label="深色" size="sm" />
                          <Radio value="system" label="跟随系统" size="sm" />
                        </RadioGroup>
                      </FormControl>
                    </Box>
                  </Stack>
                </TabPanel>

                {/* ════════════════════════════════════
                    Tab 2 — Advanced Settings
                ════════════════════════════════════ */}
                <TabPanel value={1} sx={{ p: 3 }}>
                  <Stack spacing={3}>

                    {/* Section A: Safety & Permissions */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 1.5 }}>
                        安全与权限
                      </Typography>
                      <Stack spacing={1.5}>
                        <Box
                          sx={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                          }}
                        >
                          <Typography level="body-sm">
                            允许终端命令（无需确认）
                          </Typography>
                          <Switch
                            size="sm"
                            checked={config.safety.allowTerminalCommands}
                            onChange={(e) =>
                              updateSafety('allowTerminalCommands', e.target.checked)
                            }
                          />
                        </Box>
                        <Box
                          sx={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                          }}
                        >
                          <Typography level="body-sm">
                            允许文件操作（无需确认）
                          </Typography>
                          <Switch
                            size="sm"
                            checked={config.safety.allowFileOperations}
                            onChange={(e) =>
                              updateSafety('allowFileOperations', e.target.checked)
                            }
                          />
                        </Box>
                        <Box
                          sx={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                          }}
                        >
                          <Box>
                            <Typography level="body-sm">最大连续失败次数</Typography>
                          </Box>
                          <Input
                            type="number"
                            size="sm"
                            value={config.safety.maxConsecutiveFailures}
                            onChange={(e) =>
                              updateSafety(
                                'maxConsecutiveFailures',
                                parseInt(e.target.value, 10) || 1,
                              )
                            }
                            slotProps={{ input: { min: 1, max: 20 } }}
                            sx={{ width: 80 }}
                          />
                        </Box>
                        <Box
                          sx={{
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'space-between',
                          }}
                        >
                          <Typography level="body-sm">
                            最大运行时间（分钟，0=无限）
                          </Typography>
                          <Input
                            type="number"
                            size="sm"
                            value={config.safety.maxLoopDurationMinutes}
                            onChange={(e) =>
                              updateSafety(
                                'maxLoopDurationMinutes',
                                parseInt(e.target.value, 10) || 0,
                              )
                            }
                            slotProps={{ input: { min: 0 } }}
                            sx={{ width: 80 }}
                          />
                        </Box>
                      </Stack>
                    </Box>

                    <Divider />

                    {/* Section B: Require Approval */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 0.5 }}>
                        高危操作审批列表
                      </Typography>
                      <Typography level="body-xs" color="neutral" sx={{ mb: 1.5 }}>
                        选中的操作在执行前需要用户手动确认
                      </Typography>
                      <Box
                        sx={{
                          display: 'grid',
                          gridTemplateColumns: 'repeat(2, 1fr)',
                          gap: 0.75,
                        }}
                      >
                        {BUILTIN_TOOLS.map((tool) => (
                          <Checkbox
                            key={tool}
                            size="sm"
                            checked={config.safety.requireApprovalFor.includes(tool)}
                            onChange={() => toggleApproval(tool)}
                            label={tool}
                            sx={{ fontFamily: 'code' }}
                          />
                        ))}
                      </Box>
                    </Box>

                    <Divider />

                    {/* Section C: Permanently Allowed */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 0.5 }}>
                        永久允许列表
                      </Typography>
                      <Typography level="body-xs" color="neutral" sx={{ mb: 1.5 }}>
                        这些操作类型将跳过审批弹窗，直接执行
                      </Typography>
                      {settingsStore.settings.safety.requireApprovalFor.length > 0 ? (
                        <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 0.75 }}>
                          {settingsStore.settings.safety.requireApprovalFor.map(
                            (item) => (
                              <Chip
                                key={item}
                                size="sm"
                                variant="soft"
                                color="warning"
                              >
                                {item}
                              </Chip>
                            ),
                          )}
                        </Box>
                      ) : (
                        <Typography level="body-xs" color="neutral">
                          （列表为空）
                        </Typography>
                      )}
                    </Box>

                    <Divider />

                    {/* Section D: MCP Servers */}
                    <Box>
                      <Typography level="title-sm" sx={{ mb: 1.5 }}>
                        MCP 服务器
                      </Typography>
                      {config.mcpServers.length === 0 ? (
                        <Typography level="body-xs" color="neutral">
                          暂无 MCP 服务器配置
                        </Typography>
                      ) : (
                        <Stack spacing={1}>
                          {config.mcpServers.map((server, idx) => (
                            <Sheet
                              key={idx}
                              variant="outlined"
                              sx={{
                                p: 1.5,
                                borderRadius: 'sm',
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'space-between',
                                gap: 2,
                              }}
                            >
                              <Box sx={{ minWidth: 0, flex: 1 }}>
                                <Typography level="body-sm" fontWeight="md">
                                  {server.name}
                                </Typography>
                                <Typography
                                  level="body-xs"
                                  color="neutral"
                                  sx={{
                                    fontFamily: 'code',
                                    overflow: 'hidden',
                                    textOverflow: 'ellipsis',
                                    whiteSpace: 'nowrap',
                                  }}
                                >
                                  {server.command} {server.args.join(' ')}
                                </Typography>
                              </Box>
                              <Switch
                                size="sm"
                                checked={server.enabled}
                                onChange={(e) =>
                                  updateMcpServer(idx, 'enabled', e.target.checked)
                                }
                              />
                            </Sheet>
                          ))}
                        </Stack>
                      )}
                    </Box>
                  </Stack>
                </TabPanel>
              </Tabs>
            )}
          </Box>

          {/* ── Footer ── */}
          <Box
            sx={{
              px: 3,
              py: 2,
              borderTop: '1px solid',
              borderColor: 'divider',
              display: 'flex',
              justifyContent: 'flex-end',
              gap: 1,
              flexShrink: 0,
            }}
          >
            <Button
              variant="outlined"
              color="neutral"
              size="sm"
              onClick={() => settingsStore.closeSettings()}
            >
              取消
            </Button>
            <Button
              variant="solid"
              color="primary"
              size="sm"
              onClick={saveConfig}
              loading={saving}
            >
              保存
            </Button>
          </Box>
        </ModalDialog>
      </Modal>

      <Snackbar
        open={snackbar.open}
        autoHideDuration={3000}
        onClose={() => setSnackbar((prev) => ({ ...prev, open: false }))}
        color={snackbar.color}
        variant="soft"
        anchorOrigin={{ vertical: 'bottom', horizontal: 'right' }}
      >
        {snackbar.message}
      </Snackbar>
    </>
  );
});

export { SettingsModal };
export default SettingsModal;
