import assert from "node:assert/strict";
import { test } from "node:test";
import { tuneWindow } from "../src/lib/tuning.js";

const base = { input: 30, multiplier: 4, height: 1080, quality: 1, scale: 1.5, nrResolution: 480, nrPasses: 1 };
const effects = { vfg: true, hdr: true, sr: false, nr: false };

function fixture(fps, hz = 180) {
  let active = null;
  const commands = [];
  const tested = [];
  return {
    commands,
    tested,
    invoke: async (command, args) => {
      commands.push(command);
      if (command === "test_profile") {
        active = args.settings;
        tested.push(active);
      } else if (command === "stop" || command === "end_tuning") {
        active = null;
      } else if (command === "status") {
        return active
          ? { phase: "running", overlay: true, presentFps: fps(active), captureFps: 30, latencyMs: 35, hz, input: 30 }
          : { phase: "idle" };
      }
    },
  };
}

async function run(mock, goal, options = {}) {
  const setTimer = globalThis.setTimeout;
  globalThis.setTimeout = (callback) => queueMicrotask(callback);
  try {
    return await tuneWindow({
      invoke: mock.invoke, source: { handle: 1, screen: false }, settings: base, goal, effects,
      signal: new AbortController().signal, progress: () => {}, ...options,
    });
  } finally {
    globalThis.setTimeout = setTimer;
  }
}

test("chooses the highest-quality measured profile that reaches the requested rate", async () => {
  const mock = fixture((s) => s.quality === 2 ? 104 : s.height === 1080 ? 119 : 125);
  const result = await run(mock, 120);
  assert.equal(result.reached, true);
  assert.equal(result.best.settings.height, 1080);
  assert.equal(result.best.settings.quality, 1);
  assert.equal(result.best.settings.multiplier, 4);
  assert.ok(mock.tested.every((s) => s.hdr && !s.sr && !s.nr));
  assert.equal(mock.commands.at(-1), "end_tuning");
  assert.ok(!mock.commands.includes("update_preferences"));
});

test("a display refresh limit produces a fallback even when Present is fast enough", async () => {
  const result = await run(fixture((s) => s.height === 720 ? 242 : 215), 240);
  assert.equal(result.reached, false);
  assert.equal(result.best.settings.height, 720);
});

test("tries a higher factor when the first candidates fall short", async () => {
  const mock = fixture((s) => s.multiplier === 5 ? 120 : 100);
  const result = await run(mock, 120);
  assert.equal(result.reached, true);
  assert.equal(result.best.settings.multiplier, 5);
});

test("cancellation ends the trial without saving a profile", async () => {
  const mock = fixture(() => 120);
  const cancel = new AbortController();
  const invoke = async (command, args) => {
    const result = await mock.invoke(command, args);
    if (command === "test_profile") cancel.abort();
    return result;
  };
  await assert.rejects(run(mock, 120, { invoke, signal: cancel.signal }), { name: "AbortError" });
  assert.equal(mock.commands.at(-1), "end_tuning");
  assert.ok(!mock.commands.includes("update_preferences"));
});
