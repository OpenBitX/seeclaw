import { makeAutoObservable, runInAction } from 'mobx';
import type {
  AgentStateKind,
  ActionCard,
  LoopConfig,
  Message,
  StreamChunk,
  ApprovalRequest,
  ViewportCapturedPayload,
  TodoStep,
  TodoListPayload,
  StepStartedPayload,
  StepCompletedPayload,
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
  /** TodoList steps from the planner (global reference kept for compat) */
  todoSteps: TodoStep[] = [];
  /** Index of the step currently executing (-1 = none) */
  currentStepIdx = -1;
  /** Timestamp (Date.now()) when the current task started. Null when idle. */
  taskStartedAt: number | null = null;
  /** Error message or completion summary for terminal states */
  terminalMessage: string | null = null;
  /** Current task session ID — groups messages, screenshots, and plans */
  currentTaskId: string | null = null;
  /** Message ID of the current task's inline plan message (for in-place updates) */
  private currentPlanMessageId: string | null = null;
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

  // ── Task session management ───────────────────────────────────────────

  /**
   * Start a new task session. Generates a taskId, finalizes any previous
   * task's plan. Previous messages stay in the conversation history.
   */
  startNewTask(): string {
    // Finalize any lingering plan from a previous task
    this._finalizePreviousPlan();

    const taskId = crypto.randomUUID();
    this.currentTaskId = taskId;
    this.currentPlanMessageId = null;
    this.todoSteps = [];
    this.currentStepIdx = -1;
    return taskId;
  }

  addUserMessage(content: string): void {
    const msg: Message = {
      id: crypto.randomUUID(),
      role: 'user',
      content,
      timestamp: new Date().toISOString(),
      isStreaming: false,
      taskId: this.currentTaskId ?? undefined,
    };
    this.messages.push(msg);
  }

  startAssistantMessage(): string {
    // Close any existing streaming message to prevent orphaned "正在处理" indicators
    if (this.currentStreamingId) {
      const old = this.messages.find((m) => m.id === this.currentStreamingId);
      if (old) {
        old.isStreaming = false;
        if (this.streamStartedAt !== null) {
          old.durationMs = Date.now() - this.streamStartedAt;
          this.streamStartedAt = null;
        }
        // Remove completely empty messages (no content, reasoning, actions, or screenshot)
        const isEmpty =
          !old.content &&
          !(old.reasoningContent ?? '').length &&
          !(old.actionCards?.length) &&
          !old.screenshotBase64;
        if (isEmpty) {
          const idx = this.messages.indexOf(old);
          if (idx !== -1) this.messages.splice(idx, 1);
        }
      }
      this.currentStreamingId = null;
    }

    const id = crypto.randomUUID();
    const msg: Message = {
      id,
      role: 'assistant',
      content: '',
      reasoningContent: '',
      actionCards: [],
      timestamp: new Date().toISOString(),
      isStreaming: true,
      taskId: this.currentTaskId ?? undefined,
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
        // Remove completely empty messages (e.g. from silent LLM calls)
        const isEmpty =
          !msg.content &&
          !(msg.reasoningContent ?? '').length &&
          !(msg.actionCards?.length) &&
          !msg.screenshotBase64;
        if (isEmpty) {
          const idx = this.messages.indexOf(msg);
          if (idx !== -1) this.messages.splice(idx, 1);
        }
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

      // Finalize the plan message for the current task (mark as done, stop spinners)
      this._finalizePreviousPlan();

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
            taskId: this.currentTaskId ?? undefined,
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

  // ── TodoList management (task-scoped inline messages) ────────────────

  /** Called when backend emits `todolist_updated` — creates or updates an inline plan message */
  setTodoList(payload: TodoListPayload): void {
    const steps = payload.steps.map((s) => ({
      ...s,
      status: s.status ?? 'Pending',
    })) as TodoStep[];

    // Update global reference (for backwards compat)
    this.todoSteps = steps;

    // Find or create the plan message for the current task
    if (this.currentPlanMessageId) {
      const planMsg = this.messages.find((m) => m.id === this.currentPlanMessageId);
      if (planMsg) {
        planMsg.todoSteps = steps;
        planMsg.isTodoDone = false;
        return;
      }
    }

    // Create a new inline plan message
    const planId = crypto.randomUUID();
    this.currentPlanMessageId = planId;
    this.messages.push({
      id: planId,
      role: 'tool',
      content: '',
      timestamp: new Date().toISOString(),
      isStreaming: false,
      taskId: this.currentTaskId ?? undefined,
      todoSteps: steps,
      isTodoDone: false,
    });
  }

  /** Called when backend emits `step_started` */
  setCurrentStep(payload: StepStartedPayload): void {
    this.currentStepIdx = payload.index;

    // Update in plan message
    const planMsg = this._getPlanMessage();
    if (planMsg?.todoSteps) {
      const step = planMsg.todoSteps[payload.index];
      if (step) step.status = 'InProgress';
      // Force MobX to notice the array mutation
      planMsg.todoSteps = [...planMsg.todoSteps];
    }

    // Also update global reference
    const globalStep = this.todoSteps[payload.index];
    if (globalStep) globalStep.status = 'InProgress';
  }

  /** Called when backend emits `step_completed` */
  completeStep(payload: StepCompletedPayload): void {
    const status = payload.status ?? 'Completed';

    // Update in plan message
    const planMsg = this._getPlanMessage();
    if (planMsg?.todoSteps) {
      const step = planMsg.todoSteps[payload.index];
      if (step) step.status = status;
      planMsg.todoSteps = [...planMsg.todoSteps];
    }

    // Also update global reference
    const globalStep = this.todoSteps[payload.index];
    if (globalStep) globalStep.status = status;
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
   * Task-scoped: only considers messages belonging to the current task.
   */
  handleViewportCaptured(payload: ViewportCapturedPayload): void {
    runInAction(() => {
      const taskId = this.currentTaskId;

      // Try to attach to the current streaming message first (if same task)
      if (this.currentStreamingId) {
        const streaming = this.messages.find((m) => m.id === this.currentStreamingId);
        if (streaming && (!taskId || streaming.taskId === taskId)) {
          streaming.screenshotBase64 = payload.image_base64;
          streaming.gridN = payload.grid_n;
          return;
        }
      }

      // Try to find the most recent assistant message within the current task
      if (taskId) {
        const target = this.messages
          .slice()
          .reverse()
          .find((m) => m.role === 'assistant' && m.taskId === taskId);
        if (target) {
          target.screenshotBase64 = payload.image_base64;
          target.gridN = payload.grid_n;
          return;
        }
      }

      // Fallback: create a standalone screenshot message scoped to this task
      this.messages.push({
        id: crypto.randomUUID(),
        role: 'tool',
        content: '',
        timestamp: new Date().toISOString(),
        isStreaming: false,
        screenshotBase64: payload.image_base64,
        gridN: payload.grid_n,
        taskId: taskId ?? undefined,
      });
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
    this.todoSteps = [];
    this.currentStepIdx = -1;
    this.currentTaskId = null;
    this.currentPlanMessageId = null;
  }

  // ── Private helpers ───────────────────────────────────────────────────

  private _getPlanMessage(): Message | undefined {
    if (!this.currentPlanMessageId) return undefined;
    return this.messages.find((m) => m.id === this.currentPlanMessageId);
  }

  /**
   * Mark the current plan message as finalized (isTodoDone = true).
   * Called when the task ends (done/error/idle) or a new task starts.
   */
  private _finalizePreviousPlan(): void {
    if (this.currentPlanMessageId) {
      const planMsg = this.messages.find((m) => m.id === this.currentPlanMessageId);
      if (planMsg) {
        planMsg.isTodoDone = true;
        if (planMsg.todoSteps) {
          planMsg.todoSteps = [...planMsg.todoSteps];
        }
      }
      this.currentPlanMessageId = null;
    }
  }
}

export const agentStore = new AgentStore();
