import { observer } from 'mobx-react-lite';
import { motion, AnimatePresence } from 'framer-motion';
import Box from '@mui/joy/Box';
import Chip from '@mui/joy/Chip';
import { agentStore } from '../../store/AgentStore';
import { formatElapsed } from '../../utils/format';

const STATE_LABELS: Record<string, string> = {
  idle: '就绪',
  routing: '路由中',
  observing: '感知中',
  planning: '规划中',
  executing: '执行中',
  waiting_for_user: '等待确认',
  evaluating: '评估中',
  error: '出错',
  done: '完成',
};

const STATE_COLORS: Record<string, 'neutral' | 'primary' | 'success' | 'warning' | 'danger'> = {
  idle: 'neutral',
  routing: 'primary',
  observing: 'primary',
  planning: 'primary',
  executing: 'warning',
  waiting_for_user: 'warning',
  evaluating: 'primary',
  error: 'danger',
  done: 'success',
};

/** Pulsing dot shown inside the Chip when agent is active */
function PulsingDot({ color }: { color: string }) {
  return (
    <Box
      sx={{
        position: 'relative',
        width: 8,
        height: 8,
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        mr: 0.5,
        flexShrink: 0,
      }}
    >
      <motion.div
        animate={{ scale: [1, 1.8, 1], opacity: [0.6, 0, 0.6] }}
        transition={{ duration: 1.2, repeat: Infinity, ease: 'easeInOut' }}
        style={{
          position: 'absolute',
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: color,
        }}
      />
      <Box
        sx={{
          width: 6,
          height: 6,
          borderRadius: '50%',
          background: color,
          zIndex: 1,
        }}
      />
    </Box>
  );
}

const DOT_COLORS: Record<string, string> = {
  primary: 'var(--joy-palette-primary-500)',
  warning: 'var(--joy-palette-warning-500)',
  danger: 'var(--joy-palette-danger-500)',
  success: 'var(--joy-palette-success-500)',
  neutral: 'var(--joy-palette-neutral-500)',
};

export const StatusCapsule = observer(() => {
  const { state, isRunning, elapsedMs, loopCount, failureCount } = agentStore;
  const label = STATE_LABELS[state] ?? state;
  const color = STATE_COLORS[state] ?? 'neutral';

  return (
    <motion.div
      layout
      style={{ display: 'flex', alignItems: 'center', gap: 8 }}
      transition={{ type: 'spring', stiffness: 300, damping: 30 }}
    >
      <Chip
        color={color}
        variant="soft"
        size="sm"
        sx={{ fontWeight: 600, letterSpacing: '0.02em' }}
      >
        <Box sx={{ display: 'inline-flex', alignItems: 'center' }}>
          {isRunning && <PulsingDot color={DOT_COLORS[color] ?? DOT_COLORS.neutral} />}
          <AnimatePresence mode="wait">
            <motion.span
              key={state}
              initial={{ opacity: 0, y: -4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 4 }}
              transition={{ duration: 0.15 }}
            >
              {label}
            </motion.span>
          </AnimatePresence>
        </Box>
      </Chip>

      <AnimatePresence>
        {isRunning && (
          <motion.div
            initial={{ opacity: 0, width: 0 }}
            animate={{ opacity: 1, width: 'auto' }}
            exit={{ opacity: 0, width: 0 }}
            style={{ display: 'flex', gap: 6, fontSize: '0.75rem', color: 'var(--joy-palette-text-tertiary)' }}
          >
            <span>{formatElapsed(elapsedMs)}</span>
            {loopCount > 0 && <span>· 第 {loopCount} 轮</span>}
            {failureCount > 0 && <span>· {failureCount} 次失败</span>}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
});
