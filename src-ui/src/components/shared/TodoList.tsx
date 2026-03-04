import { observer } from 'mobx-react-lite';
import { motion, AnimatePresence } from 'framer-motion';
import Box from '@mui/joy/Box';
import Typography from '@mui/joy/Typography';
import { Check, Circle, Loader, AlertTriangle, SkipForward, CheckCircle2 } from 'lucide-react';
import type { TodoStep, StepStatus } from '../../types/agent';

const STATUS_ICON: Record<StepStatus, React.ReactNode> = {
  Pending: <Circle size={14} />,
  InProgress: <Loader size={14} />,
  Completed: <Check size={14} />,
  Failed: <AlertTriangle size={14} />,
  Skipped: <SkipForward size={14} />,
};

const STATUS_COLOR: Record<StepStatus, string> = {
  Pending: 'var(--joy-palette-neutral-400)',
  InProgress: 'var(--joy-palette-primary-500)',
  Completed: 'var(--joy-palette-success-500)',
  Failed: 'var(--joy-palette-danger-500)',
  Skipped: 'var(--joy-palette-neutral-400)',
};

interface TodoListProps {
  /** The steps to display */
  steps: TodoStep[];
  /** Whether the task is finalized (completed/failed/stopped) */
  done: boolean;
}

export const TodoList = observer(({ steps, done }: TodoListProps) => {
  if (steps.length === 0) return null;

  const completed = steps.filter((s) => s.status === 'Completed').length;
  const allDone = done || completed === steps.length;

  return (
    <Box
      sx={{
        my: 1.5,
        p: 1.5,
        borderRadius: 'md',
        bgcolor: 'background.level1',
        border: '1px solid',
        borderColor: allDone ? 'success.outlinedBorder' : 'divider',
        opacity: allDone ? 0.85 : 1,
        transition: 'opacity 0.3s, border-color 0.3s',
      }}
    >
      {/* Header */}
      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 1 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.75 }}>
          {allDone && (
            <Box sx={{ color: 'success.500', display: 'flex', alignItems: 'center' }}>
              <CheckCircle2 size={14} />
            </Box>
          )}
          <Typography level="body-xs" sx={{ fontWeight: 600, color: 'text.secondary' }}>
            任务计划{allDone ? ' · 已完成' : ''}
          </Typography>
        </Box>
        <Typography level="body-xs" sx={{ color: 'text.tertiary' }}>
          {completed}/{steps.length}
        </Typography>
      </Box>

      {/* Progress bar */}
      <Box
        sx={{
          height: 3,
          borderRadius: 99,
          bgcolor: 'neutral.100',
          mb: 1.5,
          overflow: 'hidden',
        }}
      >
        <motion.div
          animate={{ width: `${(completed / steps.length) * 100}%` }}
          transition={{ type: 'spring', stiffness: 200, damping: 25 }}
          style={{
            height: '100%',
            borderRadius: 99,
            background: allDone
              ? 'var(--joy-palette-success-500)'
              : 'var(--joy-palette-primary-500)',
          }}
        />
      </Box>

      {/* Steps */}
      <Box sx={{ display: 'flex', flexDirection: 'column', gap: 0.5 }}>
        <AnimatePresence initial={false}>
          {steps.map((step) => (
            <motion.div
              key={step.index}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ type: 'spring', stiffness: 300, damping: 30, delay: step.index * 0.03 }}
            >
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 1,
                  py: 0.4,
                  px: 0.5,
                  borderRadius: 'sm',
                  bgcolor: step.status === 'InProgress' && !allDone ? 'primary.softBg' : 'transparent',
                  transition: 'background 0.2s',
                }}
              >
                {/* Icon with spin for InProgress */}
                <Box
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    color: STATUS_COLOR[step.status],
                    flexShrink: 0,
                    animation: step.status === 'InProgress' && !allDone ? 'spin 1s linear infinite' : 'none',
                    '@keyframes spin': {
                      from: { transform: 'rotate(0deg)' },
                      to: { transform: 'rotate(360deg)' },
                    },
                  }}
                >
                  {STATUS_ICON[step.status]}
                </Box>

                <Typography
                  level="body-xs"
                  sx={{
                    flex: 1,
                    color:
                      step.status === 'Completed'
                        ? 'text.tertiary'
                        : step.status === 'InProgress'
                        ? 'text.primary'
                        : 'text.secondary',
                    textDecoration: step.status === 'Completed' ? 'line-through' : 'none',
                    lineHeight: 1.4,
                  }}
                >
                  {step.description}
                </Typography>
              </Box>
            </motion.div>
          ))}
        </AnimatePresence>
      </Box>
    </Box>
  );
});
