import { useCallback, useEffect } from 'react';
import { observer } from 'mobx-react-lite';
import { useColorScheme } from '@mui/joy/styles';
import Box from '@mui/joy/Box';
import IconButton from '@mui/joy/IconButton';
import { agentStore } from './store/AgentStore';
import { settingsStore } from './store/SettingsStore';
import { useTauriEvent } from './hooks/useTauriEvent';
import { MessageList } from './components/chat/MessageList';
import { InputBar } from './components/chat/InputBar';
import { StatusCapsule } from './components/shared/StatusCapsule';
import type { StreamChunk, AgentStateKind, ApprovalRequest } from './types/agent';

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

  // Agent state changes from Rust backend
  const handleStateChange = useCallback((state: AgentStateKind) => {
    agentStore.setState(state);
  }, []);
  useTauriEvent<AgentStateKind>('agent_state_changed', handleStateChange);

  // Human-in-the-loop approval requests
  const handleApprovalRequest = useCallback((req: ApprovalRequest) => {
    agentStore.setApprovalRequest(req);
  }, []);
  useTauriEvent<ApprovalRequest>('action_required', handleApprovalRequest);

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
          {mode === 'dark' ? '☀' : '☾'}
        </IconButton>
      </Box>

      {/* Main chat area */}
      <MessageList />

      {/* Input bar */}
      <InputBar />
    </Box>
  );
});

export default function App() {
  return <AppInner />;
}
