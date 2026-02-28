import { observer } from 'mobx-react-lite';
import { motion, AnimatePresence } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import { agentStore } from '../../store/AgentStore';
import type { AgentStateKind } from '../../types/agent';

/**
 * A compact animated indicator shown inside the assistant message bubble
 * when the agent is actively working but no LLM content has arrived yet.
 *
 * Displays a state-aware label + bouncing dots animation so the user
 * knows something is happening even during silent phases (VLM, execution…).
 */

const STATE_ACTIVITY: Record<string, string> = {
  routing: '正在路由…',
  observing: '正在分析屏幕…',
  planning: '正在思考…',
  executing: '正在执行操作…',
  evaluating: '正在评估进度…',
  waiting_for_user: '等待您的确认…',
};

function labelForState(state: AgentStateKind, activity: string | null): string {
  if (activity) return activity;
  return STATE_ACTIVITY[state] ?? '正在处理…';
}

/** Three bouncing dots */
function BouncingDots() {
  return (
    <Box
      component="span"
      sx={{ display: 'inline-flex', alignItems: 'center', gap: '3px', ml: 0.5 }}
    >
      {[0, 1, 2].map((i) => (
        <motion.span
          key={i}
          animate={{ y: [0, -4, 0] }}
          transition={{
            duration: 0.5,
            repeat: Infinity,
            delay: i * 0.15,
            ease: 'easeInOut',
          }}
          style={{
            display: 'inline-block',
            width: 4,
            height: 4,
            borderRadius: '50%',
            background: 'var(--joy-palette-primary-400)',
          }}
        />
      ))}
    </Box>
  );
}

export const ThinkingIndicator = observer(() => {
  const { state, latestActivity } = agentStore;
  const label = labelForState(state, latestActivity);

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -4 }}
        transition={{ duration: 0.2 }}
      >
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            gap: 1,
            py: 0.75,
            px: 0.5,
          }}
        >
          {/* Pulsing ring icon */}
          <Box
            sx={{
              position: 'relative',
              width: 18,
              height: 18,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              flexShrink: 0,
            }}
          >
            <motion.div
              animate={{ scale: [1, 1.6, 1], opacity: [0.5, 0, 0.5] }}
              transition={{ duration: 1.5, repeat: Infinity, ease: 'easeInOut' }}
              style={{
                position: 'absolute',
                width: 14,
                height: 14,
                borderRadius: '50%',
                background: 'var(--joy-palette-primary-300)',
              }}
            />
            <Box
              sx={{
                width: 8,
                height: 8,
                borderRadius: '50%',
                bgcolor: 'primary.400',
                zIndex: 1,
              }}
            />
          </Box>

          <Typography
            level="body-sm"
            sx={{
              color: 'text.secondary',
              fontWeight: 500,
              userSelect: 'none',
            }}
          >
            {label}
            <BouncingDots />
          </Typography>
        </Box>
      </motion.div>
    </AnimatePresence>
  );
});
