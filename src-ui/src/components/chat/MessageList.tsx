import { useEffect, useRef } from 'react';
import { observer } from 'mobx-react-lite';
import { AnimatePresence, motion } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import { agentStore } from '../../store/AgentStore';
import { StreamingMessage } from './StreamingMessage';
import { ApprovalCard } from '../shared/ApprovalCard';
import { formatTimestamp } from '../../utils/format';

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
      <ApprovalCard />

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
              ) : (
                <Box sx={{ width: '100%', maxWidth: 680 }}>
                  <StreamingMessage message={msg} />
                </Box>
              )}
              <Typography level="body-xs" sx={{ mt: 0.5, color: 'text.tertiary' }}>
                {formatTimestamp(msg.timestamp)}
              </Typography>
            </Box>
          </motion.div>
        ))}
      </AnimatePresence>

      <div ref={bottomRef} />
    </Box>
  );
});
