import { useCallback, useEffect } from 'react';
import { observer } from 'mobx-react-lite';
import { useColorScheme } from '@mui/joy/styles';
import Box from '@mui/joy/Box';
import IconButton from '@mui/joy/IconButton';
import { Sun, Moon, Settings, Minus, Square, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { agentStore } from './store/AgentStore';
import { settingsStore } from './store/SettingsStore';
import { useTauriEvent } from './hooks/useTauriEvent';
import { MessageList } from './components/chat/MessageList';
import { InputBar } from './components/chat/InputBar';
import { StatusCapsule } from './components/shared/StatusCapsule';
import { SettingsModal } from './components/settings/SettingsModal';
import type {
  StreamChunk,
  AgentStatePayload,
  ApprovalRequest,
  ViewportCapturedPayload,
} from './types/agent';

// ── Window controls ───────────────────────────────────────────────────────────

const appWindow = getCurrentWindow();

async function handleMaximize() {
  const isMax = await appWindow.isMaximized();
  if (isMax) {
    await appWindow.unmaximize();
  } else {
    await appWindow.maximize();
  }
}

function WinBtn({
  title,
  danger,
  onClick,
  children,
}: {
  title: string;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Box
      component="button"
      title={title}
      onClick={onClick}
      sx={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: 28,
        height: 28,
        border: 'none',
        background: 'none',
        cursor: 'pointer',
        borderRadius: 'sm',
        color: 'text.secondary',
        transition: 'background 0.12s, color 0.12s',
        '&:hover': danger
          ? { bgcolor: 'danger.500', color: '#fff' }
          : { bgcolor: 'neutral.200', color: 'text.primary' },
      }}
    >
      {children}
    </Box>
  );
}

// ── Main app ──────────────────────────────────────────────────────────────────

const AppInner = observer(() => {
  const { mode, setMode } = useColorScheme();

  useEffect(() => {
    const { theme } = settingsStore.settings;
    setMode(theme === 'system' ? 'system' : theme);
  }, [settingsStore.settings.theme, setMode]);

  const handleStreamChunk = useCallback((chunk: StreamChunk) => {
    agentStore.handleStreamChunk(chunk);
  }, []);
  useTauriEvent<StreamChunk>('llm_stream_chunk', handleStreamChunk);

  const handleStateChange = useCallback((payload: AgentStatePayload) => {
    agentStore.setState(payload.state);
    // Pre-open an assistant message bubble for states that will stream LLM content,
    // so the "thinking" indicator appears immediately without waiting for the first chunk.
    if (payload.state === 'planning' || payload.state === 'evaluating') {
      agentStore.startAssistantMessage();
    }
  }, []);
  useTauriEvent<AgentStatePayload>('agent_state_changed', handleStateChange);

  const handleApprovalRequest = useCallback((req: ApprovalRequest) => {
    if (settingsStore.permanentlyAllowed.includes(req.action.type)) {
      invoke('confirm_action', { approved: true });
      return;
    }
    agentStore.setApprovalRequest(req);
  }, []);
  useTauriEvent<ApprovalRequest>('action_required', handleApprovalRequest);

  const handleViewportCaptured = useCallback((payload: ViewportCapturedPayload) => {
    agentStore.handleViewportCaptured(payload);
  }, []);
  useTauriEvent<ViewportCapturedPayload>('viewport_captured', handleViewportCaptured);

  /** Fine-grained activity labels emitted during execution/observation phases */
  const handleActivity = useCallback((payload: { text: string }) => {
    agentStore.setActivity(payload.text);
  }, []);
  useTauriEvent<{ text: string }>('agent_activity', handleActivity);

  // Sync MobX immediately when Rust broadcasts config_updated after save
  const handleConfigUpdated = useCallback((raw: Record<string, unknown>) => {
    settingsStore.syncFromBackend(raw);
  }, []);
  useTauriEvent<Record<string, unknown>>('config_updated', handleConfigUpdated);

  const toggleTheme = () => {
    const next = mode === 'dark' ? 'light' : 'dark';
    setMode(next);
    settingsStore.setTheme(next);
  };

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        bgcolor: 'background.body',
        color: 'text.primary',
        borderRadius: 'inherit',
        overflow: 'hidden',
      }}
    >
      {/* ── Title bar ──────────────────────────────────────────────────────── */}
      {/*
        Drag strategy:
        • An absolutely-positioned <div data-tauri-drag-region> fills the entire
          header at z-index 0 — this is the "glass" drag surface.
        • All real content sits at z-index 1, so clicks land on the content
          elements rather than the drag layer underneath.
        • Result: dragging from empty space works; clicking buttons works.
      */}
      <Box
        component="header"
        sx={{
          position: 'relative',           // needed for absolute drag overlay
          display: 'flex',
          alignItems: 'center',
          px: 2,
          gap: 1,
          bgcolor: 'background.surface',
          borderBottom: '1px solid',
          borderColor: 'divider',
          minHeight: 48,
          flexShrink: 0,
          userSelect: 'none',
        }}
      >
        {/* Drag surface — behind everything */}
        {/* @ts-ignore — Tauri drag attribute */}
        <Box
          data-tauri-drag-region
          sx={{
            position: 'absolute',
            inset: 0,
            zIndex: 0,
            cursor: 'default',
          }}
        />

        {/* Brand (z:1 — above drag layer, not draggable) */}
        <Box
          sx={{
            zIndex: 1,
            fontWeight: 700,
            fontSize: 'sm',
            letterSpacing: '0.06em',
            color: 'text.primary',
            whiteSpace: 'nowrap',
          }}
        >
          SEECLAW
        </Box>

        {/* Status capsule (z:1) */}
        <Box sx={{ zIndex: 1 }}>
          <StatusCapsule />
        </Box>

        {/* Spacer — this area IS draggable because it shows the drag layer */}
        <Box sx={{ flex: 1 }} />

        {/* Action buttons (z:1 — above drag layer, clickable) */}
        <Box sx={{ zIndex: 1, display: 'flex', alignItems: 'center', gap: 0.25 }}>
          <IconButton
            variant="plain"
            color="neutral"
            size="sm"
            onClick={toggleTheme}
            title={mode === 'dark' ? '切换亮色' : '切换暗色'}
            sx={{ borderRadius: 'sm' }}
          >
            {mode === 'dark' ? <Sun size={15} /> : <Moon size={15} />}
          </IconButton>
          <IconButton
            variant="plain"
            color="neutral"
            size="sm"
            onClick={() => settingsStore.openSettings()}
            title="设置"
            sx={{ borderRadius: 'sm' }}
          >
            <Settings size={15} />
          </IconButton>

          {/* Separator */}
          <Box sx={{ width: '1px', height: 14, bgcolor: 'divider', mx: 0.5 }} />

          {/* Window controls */}
          <WinBtn title="最小化" onClick={() => appWindow.minimize()}>
            <Minus size={12} />
          </WinBtn>
          <WinBtn title="最大化 / 还原" onClick={handleMaximize}>
            <Square size={11} />
          </WinBtn>
          <WinBtn title="关闭" danger onClick={() => appWindow.close()}>
            <X size={13} />
          </WinBtn>
        </Box>
      </Box>

      {/* Main chat area — fills remaining space */}
      <MessageList />

      {/* Input bar */}
      <InputBar />

      {/* Settings modal — portal, no backdrop blur */}
      <SettingsModal />
    </Box>
  );
});

export default function App() {
  return <AppInner />;
}
