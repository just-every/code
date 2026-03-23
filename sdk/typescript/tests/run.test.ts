import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { codexExecSpy } from "./codexExecSpy";
import { describe, expect, it } from "@jest/globals";

import {
  assistantMessage,
  responseCompleted,
  responseStarted,
  sse,
  responseFailed,
  startResponsesTestProxy,
} from "./responsesProxy";
import type { ResponsesApiRequest } from "./responsesProxy";

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
      const result = await thread.run("Hello, world!");

      const assistantItem = result.items.find((item) => item.type === "agent_message");
      expect(assistantItem).toEqual(
        expect.objectContaining({
          type: "agent_message",
          text: "Hi!",
        }),
      );
      expect(result.finalResponse).toBe("Hi!");
      expect(thread.id).toEqual(expect.any(String));
    } finally {
      cleanup();
      await close();
    }
  });

  it("sends previous items when run is called twice", async () => {
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
      const firstResult = await thread.run("first input");
      expect(firstResult.finalResponse).toBe("First response");

      const secondResult = await thread.run("second input");
      expect(secondResult.finalResponse).toBe("Second response");

      expect(requests.length).toBeGreaterThanOrEqual(2);
    } finally {
      cleanup();
      await close();
    }
  });

  it("continues the thread when run is called twice with options", async () => {
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
      const firstResult = await thread.run("first input");
      expect(firstResult.finalResponse).toBe("First response");

      const secondResult = await thread.run("second input", {
        model: "gpt-test-1",
      });
      expect(secondResult.finalResponse).toBe("Second response");

      expect(requests.length).toBeGreaterThanOrEqual(2);
      const secondRequest = requests[1];
      expect(secondRequest).toBeDefined();
      const payload: ResponsesApiRequest = secondRequest!.json;

      expect(payload.model).toBe("gpt-test-1");
    } finally {
      cleanup();
      await close();
    }
  });

  it("resumes thread by id", async () => {
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
      const firstResult = await originalThread.run("first input");
      expect(firstResult.finalResponse).toBe("First response");

      const resumedThread = client.resumeThread(originalThread.id!);
      const result = await resumedThread.run("second input");

      expect(resumedThread.id).toBe(originalThread.id);
      expect(result.finalResponse).toBe("Second response");

      expect(requests.length).toBeGreaterThanOrEqual(2);
    } finally {
      cleanup();
      await close();
    }
  });

  it("passes turn options to exec", async () => {
    const { url, close, requests } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(
          responseStarted("response_1"),
          assistantMessage("Turn options applied", "item_1"),
          responseCompleted("response_1"),
        ),
      ],
    });

    const { args: spawnArgs, restore } = codexExecSpy();
    const { client, cleanup } = createMockClient(url);

    try {
      const thread = client.startThread({
        model: "gpt-test-1",
        sandboxMode: "workspace-write",
      });
      await thread.run("apply options");

      const payload = requests[0];
      expect(payload).toBeDefined();
      const json: ResponsesApiRequest | undefined = payload?.json;

      expect(json?.model).toBe("gpt-test-1");
      expect(spawnArgs.length).toBeGreaterThan(0);
      const commandArgs = spawnArgs[0];

      expect(commandArgs).toContain("--json");
      expectPair(commandArgs, ["--sandbox", "workspace-write"]);
      expectPair(commandArgs, ["--model", "gpt-test-1"]);

    } finally {
      cleanup();
      restore();
      await close();
    }
  });
  it("runs in provided working directory", async () => {
    const { url, close } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(
          responseStarted("response_1"),
          assistantMessage("Working directory applied", "item_1"),
          responseCompleted("response_1"),
        ),
      ],
    });

    const { args: spawnArgs, restore } = codexExecSpy();
    const workingDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "codex-working-dir-"));
    const { client, cleanup } = createTestClient({
      baseUrl: url,
      apiKey: "test",
    });

    try {
      const thread = client.startThread({
        workingDirectory,
        skipGitRepoCheck: true,
      });
      await thread.run("use custom working directory");

      const commandArgs = spawnArgs[0];
      expectPair(commandArgs, ["--cd", workingDirectory]);
    } finally {
      cleanup();
      fs.rmSync(workingDirectory, { recursive: true, force: true });
      restore();
      await close();
    }
  });

  it("throws if working directory is not git and no skipGitRepoCheck is provided", async () => {
    const { url, close } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(
          responseStarted("response_1"),
          assistantMessage("Working directory applied", "item_1"),
          responseCompleted("response_1"),
        ),
      ],
    });
    const workingDirectory = fs.mkdtempSync(path.join(os.tmpdir(), "codex-working-dir-"));
    const { client, cleanup } = createTestClient({
      baseUrl: url,
      apiKey: "test",
    });

    try {
      const thread = client.startThread({
        workingDirectory,
      });
      await expect(thread.run("use custom working directory")).rejects.toThrow(
        /Not inside a trusted directory/,
      );
    } finally {
      cleanup();
      fs.rmSync(workingDirectory, { recursive: true, force: true });
      await close();
    }
  });
  it("throws ThreadRunError on turn failures", async () => {
    const { url, close } = await startResponsesTestProxy({
      statusCode: 200,
      responseBodies: [
        sse(responseStarted("response_1")),
        sse(responseFailed("rate limit exceeded")),
      ],
    });
    const { client, cleanup } = createMockClient(url);

    try {
      const thread = client.startThread();
      await expect(thread.run("fail")).rejects.toThrow("stream disconnected before completion:");
    } finally {
      cleanup();
      await close();
    }
  }, 10000); // TODO(pakrym): remove timeout
});

/**
 * Given a list of args to `codex` and a `key`, collects all `--config`
 * overrides for that key.
 */
function collectConfigValues(args: string[] | undefined, key: string): string[] {
  if (!args) {
    throw new Error("args is undefined");
  }

  const values: string[] = [];
  for (let i = 0; i < args.length; i += 1) {
    if (args[i] !== "--config") {
      continue;
    }

    const override = args[i + 1];
    if (override?.startsWith(`${key}=`)) {
      values.push(override);
    }
  }
  return values;
}

function expectPair(args: string[] | undefined, pair: [string, string]) {
  if (!args) {
    throw new Error("args is undefined");
  }
  const index = args.findIndex((arg, i) => arg === pair[0] && args[i + 1] === pair[1]);
  if (index === -1) {
    throw new Error(`Pair ${pair[0]} ${pair[1]} not found in args`);
  }
  expect(args[index + 1]).toBe(pair[1]);
}
