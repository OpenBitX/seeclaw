export type AgentStateKind =
  | 'idle'
  | 'routing'
  | 'observing'
  | 'planning'
  | 'executing'
  | 'waiting_for_user'
  | 'evaluating'
  | 'error'
  | 'done';

export interface AgentAction {
  type: string;
  [key: string]: unknown;
}

export interface ActionResult {
  action: AgentAction;
  success: boolean;
  error?: string;
  timestamp: string;
}

export interface ActionCard {
  id: string;
  action: AgentAction;
  result?: ActionResult;
  timestamp: string;
  isExpanded: boolean;
}

export interface AgentStatus {
  state: AgentStateKind;
  reasoningStream: string;
  contentStream: string;
  actionHistory: ActionCard[];
  loopConfig: LoopConfig;
  failureCount: number;
  loopCount: number;
  elapsedMs: number;
}

export interface LoopConfig {
  mode: 'until_done' | 'timed' | 'failure_limit';
  maxDurationMinutes?: number;
  maxFailures?: number;
}

export interface StreamChunk {
  kind: 'reasoning' | 'content' | 'tool_call' | 'done' | 'error';
  content: string;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  reasoningContent?: string;
  actionCards?: ActionCard[];
  timestamp: string;
  isStreaming: boolean;
}

export interface ApprovalRequest {
  id: string;
  action: AgentAction;
  reason: string;
  timestamp: string;
}
