// Writes react-blitz/tests/golden/<family>.bin from React trees; blitz-commit's golden test applies
// the same files and checks the tree each makes (<family>.tree).
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";
import type { Bridge } from "../src/bridge";
import { createRoot } from "../src/client";
import { flushSync } from "../src/dom";
import { BlitzElement, nodes } from "../src/node";

const GOLDEN = join(__dirname, "../../tests/golden");
let commits: Uint8Array[] = [];

function frame(ops: Uint32Array, strings: Uint8Array): Uint8Array {
  const out = new Uint8Array(8 + ops.byteLength + strings.byteLength);
  const v = new DataView(out.buffer);
  v.setUint32(0, ops.byteLength, true);
  v.setUint32(4, strings.byteLength, true);
  out.set(new Uint8Array(ops.buffer, ops.byteOffset, ops.byteLength), 8);
  out.set(strings, 8 + ops.byteLength);
  return out;
}

beforeEach(() => {
  commits = [];
  globalThis.__reactBlitz = {
    commit: (ops: Uint32Array, strings: Uint8Array) => {
      commits.push(frame(ops, strings));
    },
  } as unknown as Bridge;
});

afterEach(() => {
  globalThis.__reactBlitz = undefined;
});

const tick = () => new Promise((r) => setTimeout(r, 0));

/** Render each step into a fresh container (JS id 1, the document's body) and write the commits. */
async function family(name: string, steps: React.ReactNode[]): Promise<void> {
  nodes.delete(1);
  const body = new BlitzElement(1, "body", 0, true);
  const root = createRoot(body);
  for (const step of steps) {
    flushSync(() => root.render(step));
    await tick();
  }
  expect(commits.length).toBeGreaterThan(0);
  const size = commits.reduce((n, c) => n + c.length, 0);
  const all = new Uint8Array(size);
  let at = 0;
  for (const c of commits) {
    all.set(c, at);
    at += c.length;
  }
  writeFileSync(join(GOLDEN, `${name}.bin`), all);
}

test("create", () =>
  family("create", [
    <div className="row">
      hello
      <svg>
        <path d="M0 0L1 1" />
      </svg>
    </div>,
  ]));

test("attrs", () =>
  family("attrs", [
    <div className="a" id="x" data-state="open" />,
    <div className="a b" id="x" aria-label="row" />,
  ]));

test("style", () =>
  family("style", [
    <div style={{ width: 10, color: "red", ["--gap" as string]: "4px" }} />,
    <div style={{ width: 20, ["--gap" as string]: "4px" }} />,
  ]));

test("text", () => family("text", [<p>{"one"}</p>, <p>{"two é😀"}</p>]));

const List = ({ items }: { items: string[] }) => (
  <ul>
    {items.map((i) => (
      <li key={i} id={i}>
        {i === "c" && <span />}
      </li>
    ))}
  </ul>
);

test("tree", () =>
  family("tree", [<List items={["a", "b", "c"]} />, <List items={["c", "a", "d", "b"]} />, <List items={["d", "b", "e"]} />]));

const noop = () => {};
test("listen", () =>
  family("listen", [<button onClick={noop} onKeyDown={noop} />, <button onKeyDown={noop} onPointerDown={noop} />]));
