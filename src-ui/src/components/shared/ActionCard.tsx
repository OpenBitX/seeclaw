import { useState } from 'react';
import { motion } from 'framer-motion';
import Card from '@mui/joy/Card';
import Chip from '@mui/joy/Chip';
import Typography from '@mui/joy/Typography';
import IconButton from '@mui/joy/IconButton';
import Box from '@mui/joy/Box';
import type { ActionCard as ActionCardType } from '../../types/agent';

const ACTION_LABELS: Record<string, string> = {
  mouse_click: '点击',
  mouse_double_click: '双击',
  mouse_right_click: '右键',
  scroll: '滚动',
  type_text: '输入',
  hotkey: '快捷键',
  key_press: '按键',
  get_viewport: '截屏',
  execute_terminal: '终端',
  mcp_call: 'MCP',
  invoke_skill: 'Skill',
  wait: '等待',
  finish_task: '完成',
  report_failure: '失败',
};

const ACTION_COLORS: Record<string, 'neutral' | 'primary' | 'success' | 'warning' | 'danger'> = {
  execute_terminal: 'danger',
  mcp_call: 'warning',
  finish_task: 'success',
  report_failure: 'danger',
  get_viewport: 'neutral',
};

interface Props {
  card: ActionCardType;
}

export function ActionCard({ card }: Props) {
  const [expanded, setExpanded] = useState(false);
  const label = ACTION_LABELS[card.action.type] ?? card.action.type;
  const color = ACTION_COLORS[card.action.type] ?? 'neutral';
  const success = card.result?.success;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ type: 'spring', stiffness: 300, damping: 30 }}
    >
      <Card
        variant="outlined"
        size="sm"
        sx={{ mb: 0.5, cursor: 'pointer', userSelect: 'none' }}
        onClick={() => setExpanded(!expanded)}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Chip color={color} variant="soft" size="sm">
            {label}
          </Chip>
          {success !== undefined && (
            <Chip color={success ? 'success' : 'danger'} variant="soft" size="sm">
              {success ? '成功' : '失败'}
            </Chip>
          )}
          <Typography level="body-xs" sx={{ flex: 1, ml: 0.5 }} noWrap>
            {getActionSummary(card.action)}
          </Typography>
          <IconButton size="sm" variant="plain">
            {expanded ? '▲' : '▼'}
          </IconButton>
        </Box>

        {expanded && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ type: 'spring', stiffness: 300, damping: 30 }}
          >
            <Box
              component="pre"
              sx={{
                mt: 1,
                p: 1,
                borderRadius: 'sm',
                bgcolor: 'background.level1',
                fontSize: '0.75rem',
                fontFamily: 'code',
                overflow: 'auto',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-all',
              }}
            >
              {JSON.stringify(card.action, null, 2)}
            </Box>
            {card.result?.error && (
              <Typography level="body-xs" color="danger" sx={{ mt: 0.5 }}>
                {card.result.error}
              </Typography>
            )}
          </motion.div>
        )}
      </Card>
    </motion.div>
  );
}

function getActionSummary(action: ActionCardType['action']): string {
  if (action.element_id) return `#${action.element_id}`;
  if (action.text) return String(action.text).slice(0, 40);
  if (action.keys) return String(action.keys);
  if (action.key) return String(action.key);
  if (action.command) return String(action.command).slice(0, 40);
  if (action.summary) return String(action.summary).slice(0, 40);
  if (action.reason) return String(action.reason).slice(0, 40);
  return '';
}
