import { observer } from 'mobx-react-lite';
import { AnimatePresence, motion } from 'framer-motion';
import Card from '@mui/joy/Card';
import Typography from '@mui/joy/Typography';
import Button from '@mui/joy/Button';
import Box from '@mui/joy/Box';
import { invoke } from '@tauri-apps/api/core';
import { agentStore } from '../../store/AgentStore';

export const ApprovalCard = observer(() => {
  const { pendingApproval } = agentStore;

  const handleApprove = async () => {
    await invoke('confirm_action', { approved: true });
    agentStore.setApprovalRequest(null);
  };

  const handleDeny = async () => {
    await invoke('confirm_action', { approved: false });
    agentStore.setApprovalRequest(null);
  };

  return (
    <AnimatePresence>
      {pendingApproval && (
        <motion.div
          initial={{ opacity: 0, y: 16 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 16 }}
          transition={{ type: 'spring', stiffness: 300, damping: 30 }}
        >
          <Card
            variant="outlined"
            color="warning"
            sx={{ mb: 2, borderWidth: 2 }}
          >
            <Typography level="title-sm" color="warning">
              需要您的确认
            </Typography>
            <Typography level="body-sm" sx={{ mt: 0.5 }}>
              Agent 请求执行高危操作：
            </Typography>
            <Box
              component="pre"
              sx={{
                my: 1,
                p: 1,
                borderRadius: 'sm',
                bgcolor: 'background.level2',
                fontSize: '0.75rem',
                fontFamily: 'code',
                overflow: 'auto',
                whiteSpace: 'pre-wrap',
              }}
            >
              {JSON.stringify(pendingApproval.action, null, 2)}
            </Box>
            <Box sx={{ display: 'flex', gap: 1, justifyContent: 'flex-end' }}>
              <Button
                variant="outlined"
                color="neutral"
                size="sm"
                onClick={handleDeny}
              >
                拒绝
              </Button>
              <Button
                variant="solid"
                color="danger"
                size="sm"
                onClick={handleApprove}
              >
                允许执行
              </Button>
            </Box>
          </Card>
        </motion.div>
      )}
    </AnimatePresence>
  );
});
