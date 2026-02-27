import { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import IconButton from '@mui/joy/IconButton';
import type { Message } from '../../types/agent';
import { ActionCard } from '../shared/ActionCard';

interface Props {
  message: Message;
}

export function StreamingMessage({ message }: Props) {
  const [reasoningExpanded, setReasoningExpanded] = useState(false);
  const hasReasoning = (message.reasoningContent ?? '').length > 0;

  return (
    <Box sx={{ mb: 2 }}>
      {hasReasoning && (
        <Box sx={{ mb: 1 }}>
          <Box
            sx={{ display: 'flex', alignItems: 'center', gap: 0.5, mb: 0.5, cursor: 'pointer' }}
            onClick={() => setReasoningExpanded(!reasoningExpanded)}
          >
            <Typography level="body-xs" sx={{ color: 'text.tertiary', userSelect: 'none' }}>
              思考过程
            </Typography>
            <IconButton size="sm" variant="plain" sx={{ minWidth: 20, minHeight: 20 }}>
              {reasoningExpanded ? '▲' : '▼'}
            </IconButton>
          </Box>

          <AnimatePresence>
            {reasoningExpanded && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: 'auto' }}
                exit={{ opacity: 0, height: 0 }}
                transition={{ type: 'spring', stiffness: 300, damping: 30 }}
              >
                <Box
                  sx={{
                    p: 1.5,
                    borderRadius: 'sm',
                    bgcolor: 'background.level1',
                    borderLeft: '2px solid',
                    borderColor: 'neutral.outlinedBorder',
                    fontFamily: 'body',
                    fontSize: 'sm',
                    color: 'text.secondary',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                  }}
                >
                  {message.reasoningContent}
                </Box>
              </motion.div>
            )}
          </AnimatePresence>
        </Box>
      )}

      {message.content && (
        <Typography
          level="body-md"
          sx={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.7 }}
        >
          {message.content}
          {message.isStreaming && (
            <motion.span
              animate={{ opacity: [1, 0] }}
              transition={{ duration: 0.8, repeat: Infinity }}
              style={{ display: 'inline-block', width: 2, height: '1em', background: 'currentColor', marginLeft: 2, verticalAlign: 'text-bottom' }}
            />
          )}
        </Typography>
      )}

      {(message.actionCards ?? []).length > 0 && (
        <Box sx={{ mt: 1 }}>
          {(message.actionCards ?? []).map((card) => (
            <ActionCard key={card.id} card={card} />
          ))}
        </Box>
      )}
    </Box>
  );
}
