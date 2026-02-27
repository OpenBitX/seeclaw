import { makeAutoObservable, runInAction } from 'mobx';
import type {
  AgentStateKind,
  ActionCard,
  LoopConfig,
  Message,
  StreamChunk,
  ApprovalRequest,
} from '../types/agent';

class AgentStore {
  state: AgentStateKind = 'idle';
  messages: Message[] = [];
  failureCount = 0;
  loopCount = 0;
  elapsedMs = 0;
  loopConfig: LoopConfig = { mode: 'until_done' };
  pendingApproval: ApprovalRequest | null = null;

  private currentStreamingId: string | null = null;

  constructor() {
    makeAutoObservable(this);
  }

  get isRunning(): boolean {
    return this.state !== 'idle' && this.state !== 'error' && this.state !== 'done';
  }

  get lastMessage(): Message | undefined {
    return this.messages[this.messages.length - 1];
  }

  addUserMessage(content: string): void {
    const msg: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
      isStreaming: false,
    };
    this.messages.push(msg);
  }

  startAssistantMessage(): string {
    const id = crypto.randomUUID();
    const msg: Message = {
      id,
      role: 'assistant',
      content: '',
      reasoningContent: '',
      actionCards: [],
      timestamp: new Date().toISOString(),
      isStreaming: true,
    };
    this.messages.push(msg);
    this.currentStreamingId = id;
    return id;
  }

  handleStreamChunk(chunk: StreamChunk): void {
    if (!this.currentStreamingId) return;
    const msg = this.messages.find((m) => m.id === this.currentStreamingId);
    if (!msg) return;

    runInAction(() => {
      if (chunk.kind === 'reasoning') {
        msg.reasoningContent = (msg.reasoningContent ?? '') + chunk.content;
      } else if (chunk.kind === 'content') {
        msg.content += chunk.content;
      } else if (chunk.kind === 'done' || chunk.kind === 'error') {
        msg.isStreaming = false;
        this.currentStreamingId = null;
      }
    });
  }

  addActionCard(card: ActionCard): void {
    if (!this.currentStreamingId) return;
    const msg = this.messages.find((m) => m.id === this.currentStreamingId);
    if (msg) {
      msg.actionCards = [...(msg.actionCards ?? []), card];
    }
  }

  setState(state: AgentStateKind): void {
    this.state = state;
    if (state === 'idle' || state === 'done' || state === 'error') {
      if (this.currentStreamingId) {
        const msg = this.messages.find((m) => m.id === this.currentStreamingId);
        if (msg) msg.isStreaming = false;
        this.currentStreamingId = null;
      }
    }
  }

  setLoopStats(failureCount: number, loopCount: number, elapsedMs: number): void {
    this.failureCount = failureCount;
    this.loopCount = loopCount;
    this.elapsedMs = elapsedMs;
  }

  setApprovalRequest(req: ApprovalRequest | null): void {
    this.pendingApproval = req;
    if (req) {
      this.state = 'waiting_for_user';
    }
  }

  reset(): void {
    this.state = 'idle';
    this.messages = [];
    this.failureCount = 0;
    this.loopCount = 0;
    this.elapsedMs = 0;
    this.pendingApproval = null;
    this.currentStreamingId = null;
  }
}

export const agentStore = new AgentStore();
