// host.idle: a callback waits for a frame, then for what was due by then; `within` is its latest.
import { afterEach, beforeEach, expect, test } from "vitest";
import type { Bridge } from "../src/bridge";
import { cancelIdle, frame, idle } from "../src/host";

let asked = 0;

beforeEach(() => {
  asked = 0;
  globalThis.__reactBlitz = { frame: () => void asked++ } as unknown as Bridge;
});

afterEach(() => {
  globalThis.__reactBlitz = undefined;
});

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

test("an idle callback runs after the next frame, behind a timer already due", async () => {
  const ran: string[] = [];
  idle(() => ran.push("idle"));
  expect(asked).toBe(1);
  await wait(20);
  expect(ran).toEqual([]);
  setTimeout(() => ran.push("timer"), 0);
  frame({ time: 0 });
  expect(ran).toEqual([]);
  await wait(5);
  expect(ran).toEqual(["timer", "idle"]);
  frame({ time: 16 });
  await wait(5);
  expect(ran).toEqual(["timer", "idle"]);
});

test("a stopped idle callback never runs", async () => {
  let ran = 0;
  cancelIdle(idle(() => ran++, 5));
  frame({ time: 0 });
  await wait(15);
  expect(ran).toBe(0);
});

test("with no frame, an idle callback runs at its latest time, once", async () => {
  let ran = 0;
  idle(() => ran++, 10);
  await wait(30);
  expect(ran).toBe(1);
  frame({ time: 0 });
  await wait(5);
  expect(ran).toBe(1);
});

test("an idle callback added after a frame waits for a frame of its own", async () => {
  const ran: string[] = [];
  idle(() => ran.push("first"));
  frame({ time: 0 });
  idle(() => ran.push("second"));
  await wait(5);
  expect(ran).toEqual(["first"]);
  expect(asked).toBe(2);
  frame({ time: 16 });
  await wait(5);
  expect(ran).toEqual(["first", "second"]);
});
