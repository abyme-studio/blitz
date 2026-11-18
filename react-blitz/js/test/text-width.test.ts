// host.textWidth: a CSS font, as a computed style gives it, read into what the host measures with.
import { afterEach, beforeEach, expect, test } from "vitest";
import type { Bridge } from "../src/bridge";
import { textWidth } from "../src/host";

let asked: unknown[][] = [];

beforeEach(() => {
  asked = [];
  globalThis.__reactBlitz = { textWidth: (...args: unknown[]) => (asked.push(args), 42) } as unknown as Bridge;
});

afterEach(() => {
  globalThis.__reactBlitz = undefined;
});

test("a font's style, weight, size and families reach the host", () => {
  expect(textWidth("North", "italic 600 12px Inter, sans-serif")).toBe(42);
  expect(textWidth("North", "normal 400 10px \"Geist Mono\", monospace")).toBe(42);
  expect(textWidth("North", "bold 10.5px/1.2 serif")).toBe(42);
  expect(textWidth("North", "10px sans-serif")).toBe(42);
  expect(asked).toEqual([
    ["North", "Inter, sans-serif", 12, 600, true],
    ["North", '"Geist Mono", monospace', 10, 400, false],
    ["North", "serif", 10.5, 700, false],
    ["North", "sans-serif", 10, 400, false],
  ]);
});

test("a font in another form throws, saying the form", () => {
  expect(() => textWidth("North", "1em serif")).toThrow('"[style] [weight] <size>px <families>", not "1em serif"');
  expect(asked).toEqual([]);
});
