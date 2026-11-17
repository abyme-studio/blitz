// The calls into the host (Rust): the engine puts them on `globalThis.__reactBlitz` before any
// module runs; a test puts a fake there. Every read flushes the ops written so far first.
import { Encoder } from "./encoder";

export interface Bridge {
  /** One batch of ops. */
  commit(ops: Uint32Array, strings: Uint8Array): void;
  /** A node's border box in the viewport: x, y, width, height. Resolves style and layout first. */
  rect(id: number): number[];
  /** offsetWidth, offsetHeight, clientWidth, clientHeight, scrollWidth, scrollHeight, clientLeft, clientTop, offsetLeft, offsetTop. */
  metrics(id: number): number[];
  /** The node's offsetParent, 0 for none. */
  offsetParent(id: number): number;
  /** The scroll offset: x, y. */
  scroll(id: number): number[];
  scrollTo(id: number, x: number, y: number): void;
  /** A computed style property's value, by its CSS name. */
  style(id: number, property: string): string;
  focus(id: number): void;
  blur(id: number): void;
  /** The focused node, 0 for none. */
  focused(): number;
  matches(id: number, selector: string): boolean;
  /** The first node under `id` matching, 0 for none. */
  find(id: number, selector: string): number;
  findAll(id: number, selector: string): number[];
  /** Start or stop sending `kind` ("size" or "visible") for a node in each frame batch. */
  observe(id: number, kind: string, on: boolean): void;
  /** Ask for a frame batch after the next layout. */
  frame(): void;
  /** width, height, scale. */
  viewport(): number[];
  /** A node of the page's own: "document", "html", "head" or "body". */
  adopt(which: string): number;
  /** The topmost node at a viewport point, 0 for none. */
  at(x: number, y: number): number;
  /** An adopted node's tag, "" for a text node, "#document" for the document. */
  tag(id: number): string;
  attr(id: number, name: string): string | null;
  /** The ids of an adopted node's children. */
  children(id: number): number[];
  /** The page's address, as `location.href`. */
  address(): string;
  /** Whether the window has focus. */
  hasFocus(): boolean;
  /** A message to the host's log. */
  log(level: string, text: string): void;
}

declare global {
  // eslint-disable-next-line no-var
  var __reactBlitz: Bridge | undefined;
}

export function bridge(): Bridge {
  const b = globalThis.__reactBlitz;
  if (!b) throw new Error("react-blitz: no host: globalThis.__reactBlitz is not set");
  return b;
}

export const encoder = new Encoder();
let inCommit = false;
let scheduled = false;

/** Send what is written. A read calls this before it asks the host. */
export function flush(): void {
  scheduled = false;
  if (encoder.empty) return;
  const { ops, strings } = encoder.take();
  bridge().commit(ops, strings);
  if (hearers.size && !telling) {
    telling = true;
    queueMicrotask(tell);
  }
}

const hearers = new Set<() => void>();
let telling = false;

function tell(): void {
  telling = false;
  for (const fn of [...hearers]) fn();
}

/** Hear, once a task, that ops went to the host: a React commit, or DOM writes outside one. */
export function onSent(fn: () => void): () => void {
  hearers.add(fn);
  return () => void hearers.delete(fn);
}

/** Ops written outside a React commit (a library's DOM call, a passive effect's drop) go at the
 * end of the task. */
export function written(): void {
  if (inCommit || scheduled) return;
  scheduled = true;
  queueMicrotask(flush);
}

export function beginCommit(): void {
  inCommit = true;
}

export function endCommit(): void {
  inCommit = false;
  flush();
}

/** A read of the host, after flushing what is written. */
export function read(): Bridge {
  flush();
  return bridge();
}
