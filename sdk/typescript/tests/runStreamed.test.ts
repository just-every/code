import { describe, expect, it } from "@jest/globals";

import { ThreadEvent } from "../src/index";

import {
  assistantMessage,
  responseCompleted,
  responseStarted,
  sse,
  startResponsesTestProxy,
} from "./responsesProxy";

const codexExecPath = path.join(process.cwd(), "..", "..", "code-rs", "target", "debug", "code");

describe("Codex", () => {
  it("returns thread events", async () => {
    const { url, close } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [sse(responseStarted(), assistantMessage("Hi!"), responseCompleted())],
    });
    const { client, cleanup } = createMockClient(url);

    try {
      const thread = client.startThread();
      const result = await thread.runStreamed("Hello, world!");

      const events: ThreadEvent[] = [];
      for await (const event of result.events) {
        events.push(event);
      }

      const eventTypes = events.map((event) => event.type);
      expect(eventTypes).toEqual([
        "thread.started",
        "turn.started",
        "item.completed",
        "turn.completed",
      ]);

      const assistantMessage = events.find(
        (event) => event.type === "item.completed" && event.item.type === "agent_message",
      );
      expect(assistantMessage).toEqual(
        expect.objectContaining({
          type: "item.completed",
          item: expect.objectContaining({ text: "Hi!" }),
        }),
      );

      expect(thread.id).toEqual(expect.any(String));
    } finally {
      cleanup();
      await close();
    }
  });

  it("sends previous items when runStreamed is called twice", async () => {
    const { url, close, requests } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(
          responseStarted("response_1"),
          assistantMessage("First response", "item_1"),
          responseCompleted("response_1"),
        ),
        sse(
          responseStarted("response_2"),
          assistantMessage("Second response", "item_2"),
          responseCompleted("response_2"),
        ),
      ],
    });
    const { client, cleanup } = createMockClient(url);

    try {
      const thread = client.startThread();
      const firstRun = await thread.runStreamed("first input");
      await drainEvents(firstRun.events);

      const secondRun = await thread.runStreamed("second input");
      const collected: ThreadEvent[] = [];
      for await (const event of secondRun.events) {
        collected.push(event);
      }

      const finalMessage = collected.find(
        (event) => event.type === "item.completed" && event.item.type === "agent_message",
      );
      expect(finalMessage).toEqual(
        expect.objectContaining({
          type: "item.completed",
          item: expect.objectContaining({ text: "Second response" }),
        }),
      );

      expect(requests.length).toBeGreaterThanOrEqual(2);
    } finally {
      cleanup();
      await close();
    }
  });

  it("resumes thread by id when streaming", async () => {
    const { url, close, requests } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(
          responseStarted("response_1"),
          assistantMessage("First response", "item_1"),
          responseCompleted("response_1"),
        ),
        sse(
          responseStarted("response_2"),
          assistantMessage("Second response", "item_2"),
          responseCompleted("response_2"),
        ),
      ],
    });
    const { client, cleanup } = createMockClient(url);

    try {
      const originalThread = client.startThread();
      const firstRun = await originalThread.runStreamed("first input");
      await drainEvents(firstRun.events);

      const resumedThread = client.resumeThread(originalThread.id!);
      const secondRun = await resumedThread.runStreamed("second input");
      const collected: ThreadEvent[] = [];
      for await (const event of secondRun.events) {
        collected.push(event);
      }

      expect(resumedThread.id).toBe(originalThread.id);
      const finalMessage = collected.find(
        (event) => event.type === "item.completed" && event.item.type === "agent_message",
      );
      expect(finalMessage).toEqual(
        expect.objectContaining({
          type: "item.completed",
          item: expect.objectContaining({ text: "Second response" }),
        }),
      );

      expect(requests.length).toBeGreaterThanOrEqual(2);
    } finally {
      cleanup();
      await close();
    }
  });
});

async function drainEvents(events: AsyncGenerator<ThreadEvent>): Promise<void> {
  let done = false;
  do {
    done = (await events.next()).done ?? false;
  } while (!done);
}
