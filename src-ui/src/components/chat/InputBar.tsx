import { useState, useRef, useCallback } from 'react';
import { observer } from 'mobx-react-lite';
import Box from '@mui/joy/Box';
import Textarea from '@mui/joy/Textarea';
import IconButton from '@mui/joy/IconButton';
import { ArrowUp, Square } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { agentStore } from '../../store/AgentStore';

export const InputBar = observer(() => {
  const [value, setValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { isRunning } = agentStore;

  const handleSubmit = useCallback(async () => {
    const task = value.trim();
    if (!task || isRunning) return;

    setValue('');
    agentStore.addUserMessage(task);
    agentStore.setState('routing');

    try {
      await invoke('start_task', { task });
    } catch (err) {
      agentStore.setState('error');
      console.error('start_task failed:', err);
    }
  }, [value, isRunning]);

  const handleStop = useCallback(async () => {
    try {
      await invoke('stop_task');
      agentStore.setState('idle');
    } catch (err) {
      console.error('stop_task failed:', err);
    }
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <Box
      sx={{
        px: { xs: 2, md: 4 },
        py: 2,
        borderTop: '1px solid',
        borderColor: 'divider',
        bgcolor: 'background.body',
      }}
    >
      <Box
        sx={{
          display: 'flex',
          gap: 1,
          alignItems: 'flex-end',
          maxWidth: 760,
          mx: 'auto',
        }}
      >
        <Textarea
          slotProps={{ textarea: { ref: textareaRef } }}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="描述你要完成的任务（Enter 发送，Shift+Enter 换行）"
          disabled={isRunning}
          minRows={1}
          maxRows={6}
          sx={{ flex: 1, fontSize: 'sm' }}
        />
        {isRunning ? (
          <IconButton
            variant="solid"
            color="danger"
            onClick={handleStop}
            title="停止"
            sx={{ mb: 0.25 }}
          >
            <Square size={16} />
          </IconButton>
        ) : (
          <IconButton
            variant="solid"
            color="primary"
            onClick={handleSubmit}
            disabled={!value.trim()}
            title="发送"
            sx={{ mb: 0.25 }}
          >
            <ArrowUp size={16} />
          </IconButton>
        )}
      </Box>
      <Box sx={{ textAlign: 'center', mt: 0.5 }}>
        <Box component="span" sx={{ fontSize: '0.7rem', color: 'text.tertiary' }}>
          SeeClaw 会直接控制你的电脑，请确认操作安全
        </Box>
      </Box>
    </Box>
  );
});
