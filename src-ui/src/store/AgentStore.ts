import { makeAutoObservable, runInAction } from 'mobx';
import type {
  AgentStateKind,
  ActionCard,
  LoopConfig,
  Message,
  StreamChunk,
  ApprovalRequest,
  ViewportCapturedPayload,
} from '../types/agent';

class AgentStore {
  state: AgentStateKind = 'idle';
  messages: Message[] = [];
  failureCount = 0;
  loopCount = 0;
  elapsedMs = 0;
  loopConfig: LoopConfig = { mode: 'until_done' };
  pendingApproval: ApprovalRequest | null = null;
  /** Fine-grained activity label emitted by the engine (e.g. "正在截取屏幕…"). */
  latestActivity: string | null = null;
  /** Timestamp (Date.now()) when the current task started. Null when idle. */
  taskStartedAt: number | null = null;
  /** Error message or completion summary for terminal states */
  terminalMessage: string | null = null;
  private currentStreamingId: string | null = null;
  private streamStartedAt: number | null = null;

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
    this.streamStartedAt = null;
    return id;
  }

  handleStreamChunk(chunk: StreamChunk): void {
    // Auto-start a message slot if state event arrived late.
    if (!this.currentStreamingId) {
      if (chunk.kind === 'done' || chunk.kind === 'error') return;
      this.startAssistantMessage();
    }
    const msg = this.messages.find((m) => m.id === this.currentStreamingId);
    if (!msg) return;

    runInAction(() => {
      if (chunk.kind === 'reasoning') {
        if (this.streamStartedAt === null) this.streamStartedAt = Date.now();
        msg.reasoningContent = (msg.reasoningContent ?? '') + chunk.content;
      } else if (chunk.kind === 'content') {
        if (this.streamStartedAt === null) this.streamStartedAt = Date.now();
        msg.content += chunk.content;
      } else if (chunk.kind === 'done' || chunk.kind === 'error') {
        if (this.streamStartedAt !== null) {
          msg.durationMs = Date.now() - this.streamStartedAt;
          this.streamStartedAt = null;
        }
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

  setState(state: AgentStateKind, terminalMessage?: string): void {
    const wasRunning = this.isRunning;
    this.state = state;
    // Clear fine-grained activity on state transitions
    this.latestActivity = null;

    // Start client-side timer when transitioning from idle → active
    if (!wasRunning && this.isRunning) {
      this.taskStartedAt = Date.now();
      this.elapsedMs = 0;
      this.terminalMessage = null; // Clear any previous terminal message
    }

    if (state === 'idle' || state === 'done' || state === 'error') {
      // Freeze elapsed on terminal state
      if (this.taskStartedAt !== null) {
        this.elapsedMs = Date.now() - this.taskStartedAt;
        this.taskStartedAt = null;
      }

      if (terminalMessage) {
        this.terminalMessage = terminalMessage;
      }

      if (this.currentStreamingId) {
        // Race condition: `done` state arrived before the stream's Done chunk.
        // The streaming message IS the LLM response — close it and fill in the
        // summary if the content is still empty/incomplete.
        const msg = this.messages.find((m) => m.id === this.currentStreamingId);
        if (msg) {
          if (terminalMessage && !msg.content) {
            // Stream content hasn't arrived yet — use the summary directly
            msg.content = terminalMessage;
          }
          if (this.streamStartedAt !== null) {
            msg.durationMs = Date.now() - this.streamStartedAt;
            this.streamStartedAt = null;
          }
          msg.isStreaming = false;
        }
        this.currentStreamingId = null;
        // The streaming message already holds the response — no new message needed.
        return;
      }

      // No active streaming message: add a new one only if the last message
      // doesn't already show the same content (deduplication).
      if (terminalMessage) {
        const lastMsg = this.lastMessage;
        const alreadyShown =
          lastMsg &&
          lastMsg.role === 'assistant' &&
          lastMsg.content === terminalMessage;

        if (!alreadyShown) {
          this.messages.push({
            id: crypto.randomUUID(),
            role: 'assistant',
            content: terminalMessage,
            timestamp: new Date().toISOString(),
            isStreaming: false,
          });
        }
      }
    }
  }

  /** Called by a setInterval in StatusCapsule to keep elapsed time ticking. */
  tickElapsed(): void {
    if (this.taskStartedAt !== null) {
      this.elapsedMs = Date.now() - this.taskStartedAt;
    }
  }

  /** Called by `agent_activity` Tauri events to show fine-grained progress labels. */
  setActivity(text: string): void {
    this.latestActivity = text;
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

  /**
   * Called when the backend emits `viewport_captured`.
   * Attaches the screenshot to the most recent assistant message (or creates a tool message).
   */
  handleViewportCaptured(payload: ViewportCapturedPayload): void {
    runInAction(() => {
      // Try to attach to the current streaming message first
      const target = this.currentStreamingId
        ? this.messages.find((m) => m.id === this.currentStreamingId)
        : this.messages.slice().reverse().find((m) => m.role === 'assistant');

      if (target) {
        target.screenshotBase64 = payload.image_base64;
        target.gridN = payload.grid_n;
      } else {
        // No assistant message yet — create a standalone tool message to show the screenshot
        this.messages.push({
          id: crypto.randomUUID(),
          role: 'tool',
          content: '',
          timestamp: new Date().toISOString(),
          isStreaming: false,
          screenshotBase64: payload.image_base64,
          gridN: payload.grid_n,
        });
      }
    });
  }

  reset(): void {
    this.state = 'idle';
    this.messages = [];
    this.failureCount = 0;
    this.loopCount = 0;
    this.elapsedMs = 0;
    this.taskStartedAt = null;
    this.pendingApproval = null;
    this.latestActivity = null;
    this.terminalMessage = null;
    this.currentStreamingId = null;
    this.streamStartedAt = null;
  }
}

export const agentStore = new AgentStore();
