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
    // No top border — blends into the parchment content area
    <Box
      sx={{
        px: { xs: 2, sm: 3, md: 5 },
        pt: 1.5,
        pb: 1.5,
        bgcolor: 'background.body',
      }}
    >
      <Box sx={{ maxWidth: 740, mx: 'auto' }}>
        {/* Gemini-style floating input card */}
        <Box
          sx={{
            borderRadius: '22px',
            bgcolor: '#FDFDFD',
            // Layered natural shadow — subtle ambient + soft directional lift
            boxShadow:
              '0 1px 2px rgba(0,0,0,0.04), ' +
              '0 2px 8px rgba(0,0,0,0.06), ' +
              '0 6px 24px rgba(0,0,0,0.08)',
            border: '1px solid rgba(0,0,0,0.06)',
            transition: 'box-shadow 0.2s ease',
            '&:focus-within': {
              boxShadow:
                '0 1px 2px rgba(0,0,0,0.04), ' +
                '0 4px 12px rgba(0,0,0,0.08), ' +
                '0 8px 32px rgba(0,0,0,0.12)',
            },
          }}
        >
          <Box
            sx={{
              display: 'flex',
              alignItems: 'flex-end',
              px: 2.5,
              pt: 1.5,
              pb: 1.5,
              gap: 1,
            }}
          >
            {/* Borderless transparent textarea inside the card */}
            <Textarea
              slotProps={{ textarea: { ref: textareaRef } }}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="描述你要完成的任务（Enter 发送，Shift+Enter 换行）"
              disabled={isRunning}
              minRows={1}
              maxRows={6}
              variant="plain"
              sx={{
                flex: 1,
                fontSize: 'sm',
                bgcolor: 'transparent',
                '--Textarea-focusedHighlight': 'transparent',
                '--Textarea-focusedThickness': '0px',
                p: 0,
                '& textarea': {
                  p: 0,
                  resize: 'none',
                },
              }}
            />

            {/* Send / Stop button */}
            {isRunning ? (
              <IconButton
                variant="solid"
                color="danger"
                size="sm"
                onClick={handleStop}
                title="停止"
                sx={{ borderRadius: '50%', flexShrink: 0 }}
              >
                <Square size={15} />
              </IconButton>
            ) : (
              <IconButton
                variant="solid"
                color="neutral"
                size="sm"
                onClick={handleSubmit}
                disabled={!value.trim()}
                title="发送"
                sx={{
                  borderRadius: '50%',
                  flexShrink: 0,
                  bgcolor: value.trim() ? 'text.primary' : 'neutral.300',
                  color: '#fff',
                  '&:hover': { bgcolor: value.trim() ? 'neutral.800' : 'neutral.300' },
                  '&:disabled': { bgcolor: 'neutral.200', color: 'neutral.400' },
                }}
              >
                <ArrowUp size={15} />
              </IconButton>
            )}
          </Box>
        </Box>

        {/* Disclaimer */}
        <Box sx={{ textAlign: 'center', mt: 0.75 }}>
          <Box component="span" sx={{ fontSize: '0.68rem', color: 'text.tertiary' }}>
            SeeClaw 会直接控制你的电脑，请确认操作安全
          </Box>
        </Box>
      </Box>
    </Box>
  );
});
