import { CodexOptions } from "./codexOptions";
import { ThreadEvent, ThreadError, Usage } from "./events";
import { CodexExec } from "./exec";
import { ThreadItem } from "./items";
import { ThreadOptions } from "./threadOptions";

/** Completed turn. */
export type Turn = {
  items: ThreadItem[];
  finalResponse: string;
  usage: Usage | null;
};

/** Alias for `Turn` to describe the result of `run()`. */
export type RunResult = Turn;

/** The result of the `runStreamed` method. */
export type StreamedTurn = {
  events: AsyncGenerator<ThreadEvent>;
};

/** Alias for `StreamedTurn` to describe the result of `runStreamed()`. */
export type RunStreamedResult = StreamedTurn;

/** An input to send to the agent. */
export type Input = string;

/** Respesent a thread of conversation with the agent. One thread can have multiple consecutive turns. */
export class Thread {
  private _exec: CodexExec;
  private _options: CodexOptions;
  private _id: string | null;
  private _threadOptions: ThreadOptions;

  /** Returns the ID of the thread. Populated after the first turn starts. */
  public get id(): string | null {
    return this._id;
  }

  /* @internal */
  constructor(
    exec: CodexExec,
    options: CodexOptions,
    threadOptions: ThreadOptions,
    id: string | null = null,
  ) {
    this._exec = exec;
    this._options = options;
    this._id = id;
    this._threadOptions = threadOptions;
  }

  /** Provides the input to the agent and streams events as they are produced during the turn. */
  async runStreamed(input: string, options?: ThreadOptions): Promise<StreamedTurn> {
    return { events: this.runStreamedInternal(input, options) };
  }

  private async *runStreamedInternal(
    input: string,
    options?: ThreadOptions,
  ): AsyncGenerator<ThreadEvent> {
    const mergedOptions = {
      ...this._threadOptions,
      ...options,
    };
    if (options) {
      this._threadOptions = { ...mergedOptions };
    }
    const generator = this._exec.run({
      input,
      baseUrl: this._options.baseUrl,
      apiKey: this._options.apiKey,
      threadId: this._id,
      model: mergedOptions?.model,
      sandboxMode: mergedOptions?.sandboxMode,
      workingDirectory: mergedOptions?.workingDirectory,
      skipGitRepoCheck: mergedOptions?.skipGitRepoCheck,
    });

    for await (const item of generator) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(item);
      } catch {
        continue;
      }

      const threadEvents = mapCliEventToThreadEvents(parsed, this.id);
      if (threadEvents.length === 0) {
        continue;
      }

      for (const event of threadEvents) {
        if (event.type === "thread.started") {
          this._id = event.thread_id;
        }
        yield event;
      }
    }
  }

  /** Provides the input to the agent and returns the completed turn. */
  async run(input: string, options?: ThreadOptions): Promise<Turn> {
    const generator = this.runStreamedInternal(input, options);
    const items: ThreadItem[] = [];
    let finalResponse: string = "";
    let usage: Usage | null = null;
    let turnFailure: ThreadError | null = null;
    for await (const event of generator) {
      if (event.type === "item.completed") {
        if (event.item.type === "agent_message") {
          finalResponse = event.item.text;
        }
        items.push(event.item);
      } else if (event.type === "turn.completed") {
        usage = event.usage;
      } else if (event.type === "turn.failed") {
        turnFailure = event.error;
        break;
      }
    }
    if (turnFailure) {
      throw new Error(turnFailure.message);
    }
    return { items, finalResponse, usage };
  }
}

function mapCliEventToThreadEvents(
  raw: unknown,
  currentThreadId: string | null,
): ThreadEvent[] {
  if (typeof raw !== "object" || raw === null) {
    return [];
  }

  const event = raw as Record<string, unknown>;
  const msg = event.msg as Record<string, unknown> | undefined;
  const msgType = typeof msg?.type === "string" ? (msg.type as string) : undefined;

  if (!msgType || !msg) {
    return [];
  }

  switch (msgType) {
    case "session_configured": {
      const threadId = deriveThreadId(event, msg, currentThreadId);
      return [
        { type: "thread.started", thread_id: threadId },
      ];
    }
    case "task_started": {
      const events: ThreadEvent[] = [];
      if (!currentThreadId) {
        const derivedThreadId = deriveThreadId(event, msg, currentThreadId);
        events.push({ type: "thread.started", thread_id: derivedThreadId });
      }
      events.push({ type: "turn.started" });
      return events;
    }
    case "agent_reasoning": {
      const text = typeof msg?.text === "string" ? (msg.text as string) : "";
      const reasoningItem: ThreadItem = {
        id: extractEventId(event),
        type: "reasoning",
        text,
      };
      return [
        {
          type: "item.completed",
          item: reasoningItem,
        },
      ];
    }
    case "agent_message": {
      const message = typeof msg?.message === "string" ? (msg.message as string) : "";
      const assistantItem: ThreadItem = {
        id: extractEventId(event),
        type: "agent_message",
        text: message,
      };
      return [
        {
          type: "item.completed",
          item: assistantItem,
        },
      ];
    }
    case "token_count": {
      const info = msg?.info as { total_token_usage?: Record<string, unknown> } | undefined;
      const usageInfo = info?.total_token_usage ?? {};
      const usage = normalizeUsage(usageInfo);
      return [
        {
          type: "turn.completed",
          usage,
        },
      ];
    }
    case "error": {
      const message = typeof msg?.message === "string" ? (msg.message as string) : "Unknown error";
      return [
        {
          type: "error",
          message,
        },
      ];
    }
    default:
      return [];
  }
}

function deriveThreadId(
  event: Record<string, unknown>,
  msg: Record<string, unknown>,
  currentThreadId: string | null,
): string {
  if (typeof msg.session_id === "string" && msg.session_id.length > 0) {
    return msg.session_id;
  }

  const explicitId = event.thread_id;
  if (typeof explicitId === "string" && explicitId.length > 0) {
    return explicitId;
  }

  if (currentThreadId) {
    return currentThreadId;
  }

  const eventId = event.id;
  if (typeof eventId === "string" && eventId.length > 0) {
    return eventId;
  }

  return `thread-${Date.now()}`;
}

function extractEventId(event: Record<string, unknown>): string {
  const identifier = event.id;
  if (typeof identifier === "string" && identifier.length > 0) {
    return identifier;
  }
  const orderSequence = typeof event.event_seq === "number" ? event.event_seq : Date.now();
  return `event-${orderSequence}`;
}

function normalizeUsage(raw: Record<string, unknown>): {
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
} {
  const toNumber = (value: unknown): number => {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
    if (typeof value === "string") {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : 0;
    }
    return 0;
  };

  return {
    input_tokens: toNumber(raw.input_tokens),
    cached_input_tokens: toNumber(raw.cached_input_tokens),
    output_tokens: toNumber(raw.output_tokens),
  };
}
