import { useEffect, useRef } from 'react';
import { observer } from 'mobx-react-lite';
import { AnimatePresence, motion } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import { agentStore } from '../../store/AgentStore';
import { StreamingMessage } from './StreamingMessage';
import { ApprovalCard } from '../shared/ApprovalCard';
import { formatTimestamp, formatDuration } from '../../utils/format';

export const MessageList = observer(() => {
  const { messages } = agentStore;
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, messages[messages.length - 1]?.content]);

  if (messages.length === 0) {
    return (
      <Box
        sx={{
          flex: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexDirection: 'column',
          gap: 1,
          color: 'text.tertiary',
        }}
      >
        <Typography level="h4" sx={{ fontWeight: 300 }}>
          SeeClaw
        </Typography>
        <Typography level="body-sm">
          告诉我你需要在电脑上完成什么任务
        </Typography>
      </Box>
    );
  }

  return (
    <Box
      sx={{
        flex: 1,
        overflowY: 'auto',
        px: { xs: 2, md: 4 },
        py: 3,
        display: 'flex',
        flexDirection: 'column',
        gap: 0,
      }}
    >
      <AnimatePresence initial={false}>
        {messages.map((msg) => (
          <motion.div
            key={msg.id}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ type: 'spring', stiffness: 300, damping: 30 }}
          >
            <Box
              sx={{
                mb: 3,
                display: 'flex',
                flexDirection: 'column',
                alignItems: msg.role === 'user' ? 'flex-end' : 'flex-start',
              }}
            >
              {msg.role === 'user' ? (
                <Box
                  sx={{
                    maxWidth: '70%',
                    bgcolor: 'background.level2',
                    borderRadius: 'lg',
                    px: 2,
                    py: 1.5,
                  }}
                >
                  <Typography level="body-md" sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                    {msg.content}
                  </Typography>
                </Box>
              ) : msg.role === 'tool' && msg.screenshotBase64 ? (
                <Box sx={{ width: '100%', maxWidth: 680 }}>
                  <Typography level="body-xs" sx={{ color: 'text.tertiary', mb: 0.5 }}>
                    视野快照{msg.gridN ? ` · ${msg.gridN}×${msg.gridN} 网格` : ''}
                  </Typography>
                  <Box
                    component="img"
                    src={`data:image/png;base64,${msg.screenshotBase64}`}
                    alt="viewport screenshot"
                    sx={{
                      width: '100%',
                      maxWidth: 640,
                      borderRadius: 'sm',
                      border: '1px solid',
                      borderColor: 'divider',
                      display: 'block',
                    }}
                  />
                </Box>
              ) : (
                <Box sx={{ width: '100%', maxWidth: 680 }}>
                  <StreamingMessage message={msg} />
                </Box>
              )}
              <Typography level="body-xs" sx={{ mt: 0.5, color: 'text.tertiary', display: 'flex', gap: 0.75 }}>
                <span>{formatTimestamp(msg.timestamp)}</span>
                {msg.role === 'assistant' && msg.durationMs !== undefined && (
                  <span style={{ opacity: 0.6 }}>{formatDuration(msg.durationMs)}</span>
                )}
              </Typography>
            </Box>
          </motion.div>
        ))}
      </AnimatePresence>

      <ApprovalCard />

      <div ref={bottomRef} />
    </Box>
  );
});
