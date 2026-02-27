import { useCallback, useEffect } from 'react';
import { observer } from 'mobx-react-lite';
import { useColorScheme } from '@mui/joy/styles';
import Box from '@mui/joy/Box';
import IconButton from '@mui/joy/IconButton';
import { Sun, Moon, Settings } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { agentStore } from './store/AgentStore';
import { settingsStore } from './store/SettingsStore';
import { useTauriEvent } from './hooks/useTauriEvent';
import { MessageList } from './components/chat/MessageList';
import { InputBar } from './components/chat/InputBar';
import { StatusCapsule } from './components/shared/StatusCapsule';
import { SettingsModal } from './components/settings/SettingsModal';
import type { StreamChunk, AgentStateKind, AgentStatePayload, ApprovalRequest, ViewportCapturedPayload } from './types/agent';

const AppInner = observer(() => {
  const { mode, setMode } = useColorScheme();

  // Sync theme from settings store
  useEffect(() => {
    const { theme } = settingsStore.settings;
    if (theme === 'system') {
      setMode('system');
    } else {
      setMode(theme);
    }
  }, [settingsStore.settings.theme, setMode]);

  // LLM streaming chunks from Rust backend
  const handleStreamChunk = useCallback((chunk: StreamChunk) => {
    agentStore.handleStreamChunk(chunk);
  }, []);
  useTauriEvent<StreamChunk>('llm_stream_chunk', handleStreamChunk);

  // Agent state changes from Rust backend.
  // Rust serializes AgentState as { state: "idle" } (serde tag), so we extract .state here.
  const handleStateChange = useCallback((payload: AgentStatePayload) => {
    agentStore.setState(payload.state);
    // Open a streaming message slot as soon as planning begins.
    if (payload.state === 'planning') {
      agentStore.startAssistantMessage();
    }
  }, []);
  useTauriEvent<AgentStatePayload>('agent_state_changed', handleStateChange);

  // Human-in-the-loop approval requests
  const handleApprovalRequest = useCallback((req: ApprovalRequest) => {
    if (settingsStore.permanentlyAllowed.includes(req.action.type)) {
      invoke('confirm_action', { approved: true });
      return;
    }
    agentStore.setApprovalRequest(req);
  }, []);
  useTauriEvent<ApprovalRequest>('action_required', handleApprovalRequest);

  // Viewport screenshot captured by get_viewport
  const handleViewportCaptured = useCallback((payload: ViewportCapturedPayload) => {
    agentStore.handleViewportCaptured(payload);
  }, []);
  useTauriEvent<ViewportCapturedPayload>('viewport_captured', handleViewportCaptured);

  const toggleTheme = () => {
    setMode(mode === 'dark' ? 'light' : 'dark');
  };

  return (
    <Box
      sx={{
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        bgcolor: 'background.body',
        color: 'text.primary',
      }}
    >
      {/* Header */}
      <Box
        component="header"
        sx={{
          display: 'flex',
          alignItems: 'center',
          px: { xs: 2, md: 4 },
          py: 1.5,
          borderBottom: '1px solid',
          borderColor: 'divider',
          gap: 2,
          bgcolor: 'background.surface',
          minHeight: 52,
        }}
      >
        <Box sx={{ fontWeight: 700, fontSize: 'sm', letterSpacing: '0.05em', color: 'text.primary' }}>
          SEECLAW
        </Box>
        <Box sx={{ flex: 1 }}>
          <StatusCapsule />
        </Box>
        <IconButton
          variant="plain"
          color="neutral"
          size="sm"
          onClick={toggleTheme}
          title={mode === 'dark' ? '切换亮色模式' : '切换暗色模式'}
        >
          {mode === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
        </IconButton>
        <IconButton
          variant="plain"
          color="neutral"
          size="sm"
          onClick={() => settingsStore.openSettings()}
          title="设置"
        >
          <Settings size={16} />
        </IconButton>
      </Box>

      {/* Main chat area */}
      <MessageList />

      {/* Input bar */}
      <InputBar />
      <SettingsModal />
    </Box>
  );
});

export default function App() {
  return <AppInner />;
}
