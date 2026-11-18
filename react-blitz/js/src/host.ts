// react-blitz/host: the host calls. `@abyme/host`'s native half and the DOM-shaped members of
// react-blitz's nodes both call these, so each has one owner. Reads flush the ops written so far,
// so a style write and a read in one task see the new layout.
import { onSent, read } from "./bridge";
import { BlitzElement, BlitzNode, BlitzRect, BlitzText, cssName, nodes } from "./node";
import { frameTimed, timed } from "./times";

/** The node the host calls `id`, made when JS has not seen it (a node of the page's own). */
export function node(id: number): BlitzNode | null {
  if (!id) return null;
  const known = nodes.get(id);
  if (known) return known;
  const tag = read().tag(id);
  if (tag === "#document") return (globalThis as any).document ?? null;
  return tag === "" ? new BlitzText(id, "", true) : new BlitzElement(id, tag, 0, true);
}

function element(id: number): BlitzElement | null {
  const n = node(id);
  return n instanceof BlitzElement ? n : null;
}

export function rect(el: BlitzNode): BlitzRect {
  const [x, y, w, h] = read().rect(el.rbId);
  return new BlitzRect(x, y, w, h);
}

/** offsetWidth, offsetHeight, clientWidth, clientHeight, scrollWidth, scrollHeight, clientLeft, clientTop, offsetLeft, offsetTop. */
export function metrics(el: BlitzNode): number[] {
  return read().metrics(el.rbId);
}

export function offsetParent(el: BlitzNode): BlitzElement | null {
  return element(read().offsetParent(el.rbId));
}

export function scrollPosition(el: BlitzNode): { x: number; y: number } {
  const [x, y] = read().scroll(el.rbId);
  return { x: x!, y: y! };
}

export function scrollTo(el: BlitzNode, to: { top?: number; left?: number }): void {
  const at = scrollPosition(el);
  read().scrollTo(el.rbId, to.left ?? at.x, to.top ?? at.y);
}

/** Scroll each scrolling ancestor so the element shows, as `scrollIntoView` with `block: "nearest"`. */
export function scrollInto(el: BlitzElement, opts?: boolean | { block?: string; inline?: string }): void {
  const block = typeof opts === "object" ? opts.block ?? "start" : opts === false ? "end" : "start";
  const r = rect(el);
  for (let p = el.parentElement; p; p = p.parentElement) {
    const m = metrics(p);
    if (m[5]! <= m[3]! && m[4]! <= m[2]!) continue;
    const pr = rect(p);
    const at = scrollPosition(p);
    const top = pr.y + m[7]!;
    const height = m[3]!;
    let dy = 0;
    if (block === "start") dy = r.y - top;
    else if (block === "end") dy = r.bottom - (top + height);
    else if (block === "center") dy = r.y + r.height / 2 - (top + height / 2);
    else if (r.y < top) dy = r.y - top;
    else if (r.bottom > top + height) dy = Math.min(r.bottom - (top + height), r.y - top);
    if (dy) scrollTo(p, { top: at.y + dy });
    return;
  }
}

/** The computed style, read property by property as asked. */
export function computedStyle(el: BlitzNode): Record<string, any> & { getPropertyValue(p: string): string } {
  const get = (p: string) => read().style(el.rbId, p);
  return new Proxy({ getPropertyValue: get } as any, {
    get(target, key) {
      if (key in target) return target[key];
      if (typeof key !== "string") return undefined;
      return get(cssName(key));
    },
  });
}

export function focus(el: BlitzNode): void {
  read().focus(el.rbId);
}

export function blur(el?: BlitzNode): void {
  const b = read();
  const id = el?.rbId ?? b.focused();
  if (id) b.blur(id);
}

export function focused(): BlitzElement | null {
  return element(read().focused());
}

export function hasFocus(): boolean {
  return read().hasFocus();
}

export function matches(el: BlitzNode, selector: string): boolean {
  return read().matches(el.rbId, selector);
}

export function find(root: BlitzNode, selector: string): BlitzElement | null {
  return element(read().find(root.rbId, selector));
}

export function findAll(root: BlitzNode, selector: string): BlitzElement[] {
  return read()
    .findAll(root.rbId, selector)
    .map(element)
    .filter((e): e is BlitzElement => e !== null);
}

export function attr(el: BlitzNode, name: string): string | null {
  return read().attr(el.rbId, name);
}

/** The children of a node of the page's own, as the host has them. */
export function children(el: BlitzNode): BlitzNode[] {
  return read()
    .children(el.rbId)
    .map(node)
    .filter((n): n is BlitzNode => n !== null);
}

export function at(x: number, y: number): BlitzElement | null {
  return element(read().at(x, y));
}

export function viewport(): { width: number; height: number; scale: number } {
  const [width, height, scale] = read().viewport();
  return { width: width!, height: height!, scale: scale! };
}

/** A CSS `font` as a canvas takes it, in the form a computed style gives: `[style] [weight] <size>px <families>`. */
const FONT = /^\s*(?:(normal|italic|oblique)\s+)?(?:(normal|bold|\d{1,4})\s+)?(\d*\.?\d+)px(?:\/\S+)?\s+(\S.*)$/;

/**
 * The width `text` takes on one line in `font`, in CSS pixels: a canvas's
 * `measureText(text).width`. `font` is a CSS font of the form `[style] [weight] <size>px
 * <families>`, as `italic 600 12px Inter, sans-serif`; another form throws.
 */
export function textWidth(text: string, font: string): number {
  const m = FONT.exec(font);
  if (!m) throw new Error(`react-blitz: textWidth reads a font as "[style] [weight] <size>px <families>", not "${font}"`);
  const weight = m[2] === "bold" ? 700 : m[2] === undefined || m[2] === "normal" ? 400 : Number(m[2]);
  return read().textWidth(text, m[4]!.trim(), Number(m[3]), weight, m[1] === "italic" || m[1] === "oblique");
}

/** A node of the page's own: "document", "html", "head" or "body". */
export function adopt(which: "document" | "html" | "head" | "body"): BlitzNode {
  const n = node(read().adopt(which));
  if (!n) throw new Error(`react-blitz: the page has no ${which}`);
  return n;
}

export function log(level: string, text: string): void {
  read().log(level, text);
}

// Observations arrive once a frame, after layout.

export interface SizeEntry {
  target: BlitzNode;
  contentRect: BlitzRect;
  borderBoxSize: { inlineSize: number; blockSize: number }[];
  contentBoxSize: { inlineSize: number; blockSize: number }[];
  devicePixelContentBoxSize: { inlineSize: number; blockSize: number }[];
}
export interface VisibleEntry {
  target: BlitzNode;
  isIntersecting: boolean;
  intersectionRatio: number;
  boundingClientRect: BlitzRect;
  intersectionRect: BlitzRect;
  rootBounds: BlitzRect | null;
  time: number;
}

const sizeWatchers = new Map<number, Set<(e: SizeEntry) => void>>();
const visibleWatchers = new Map<number, Set<(e: VisibleEntry) => void>>();

function watch<E>(map: Map<number, Set<(e: E) => void>>, kind: string, el: BlitzNode, fn: (e: E) => void): () => void {
  let set = map.get(el.rbId);
  if (!set) {
    set = new Set();
    map.set(el.rbId, set);
    read().observe(el.rbId, kind, true);
  }
  set.add(fn);
  return () => {
    const s = map.get(el.rbId);
    if (!s?.delete(fn) || s.size) return;
    map.delete(el.rbId);
    read().observe(el.rbId, kind, false);
  };
}

/** Hear each change of an element's size, the first in the next frame. */
export function observeSize(el: BlitzNode, fn: (e: SizeEntry) => void): () => void {
  return watch(sizeWatchers, "size", el, fn);
}

/** Hear each change of whether an element shows in the viewport, the first in the next frame. */
export function observeVisible(el: BlitzNode, fn: (e: VisibleEntry) => void): () => void {
  return watch(visibleWatchers, "visible", el, fn);
}

/** What a change watch hears: the element watched. react-blitz does not say what changed. */
export interface ChangeRecord {
  type: "childList";
  target: BlitzNode;
  addedNodes: BlitzNode[];
  removedNodes: BlitzNode[];
}

/** Hear that the tree may have changed under `el`: once a task in which ops went to the host, while
 * `el` is in the document. Coarser than a MutationObserver, which says what changed and where. */
export function observeChanges(el: BlitzNode, fn: (records: ChangeRecord[]) => void): () => void {
  return onSent(() => {
    if (el.isConnected) fn([{ type: "childList", target: el, addedNodes: [], removedNodes: [] }]);
  });
}

let frameCallbacks = new Map<number, (t: number) => void>();
let nextHandle = 1;

/** Run `fn` once before the next paint. Answers a handle `cancelFrame` takes. */
export function nextFrame(fn: (t: number) => void): number {
  const handle = nextHandle++;
  if (frameCallbacks.size === 0) read().frame();
  frameCallbacks.set(handle, fn);
  return handle;
}

export function cancelFrame(handle: number): void {
  frameCallbacks.delete(handle);
}

let idleCallbacks = new Map<number, { fn: () => void; timer?: ReturnType<typeof setTimeout> }>();

/** Run `fn` once the page is idle: after the next frame, when what was due by then has run, as a
 * browser's `requestIdleCallback` does; at `within` ms at the latest. Answers a handle `cancelIdle`
 * takes. */
export function idle(fn: () => void, within?: number): number {
  const handle = nextHandle++;
  if (idleCallbacks.size === 0) read().frame();
  const timer = within === undefined ? undefined : setTimeout(() => runIdle([handle]), within);
  idleCallbacks.set(handle, { fn, timer });
  return handle;
}

export function cancelIdle(handle: number): void {
  clearTimeout(idleCallbacks.get(handle)?.timer);
  idleCallbacks.delete(handle);
}

function runIdle(handles: number[]): void {
  for (const h of handles) {
    const waiting = idleCallbacks.get(h);
    if (!waiting) continue;
    cancelIdle(h);
    timed("idle callback", waiting.fn, waiting.fn);
  }
  // One added since the frame waits for a frame of its own.
  if (idleCallbacks.size) read().frame();
}

/** What the host sends once a frame, after layout. */
export interface FrameBatch {
  time: number;
  /** id, border width, border height, content width, content height */
  sizes?: number[][];
  /** id, showing (1 or 0), ratio, then the border box: x, y, width, height */
  visible?: number[][];
  /** id, x, y */
  scroll?: number[][];
  /** id, type, animation or property name, elapsed seconds */
  ends?: [number, string, string, number][];
}

/** The host's frame: observations first, then the frame callbacks. */
export function frame(batch: FrameBatch): void {
  for (const [id, bw, bh, cw, ch] of batch.sizes ?? []) {
    const n = nodes.get(id!);
    const fns = sizeWatchers.get(id!);
    if (!n || !fns) continue;
    const entry: SizeEntry = {
      target: n,
      contentRect: new BlitzRect(0, 0, cw, ch),
      contentBoxSize: [{ inlineSize: cw!, blockSize: ch! }],
      borderBoxSize: [{ inlineSize: bw!, blockSize: bh! }],
      devicePixelContentBoxSize: [{ inlineSize: cw!, blockSize: ch! }],
    };
    for (const fn of [...fns]) timed("size watcher", fn, () => fn(entry));
  }
  for (const [id, on, ratio, x, y, w, h] of batch.visible ?? []) {
    const n = nodes.get(id!);
    const fns = visibleWatchers.get(id!);
    if (!n || !fns) continue;
    const box = new BlitzRect(x, y, w, h);
    const vp = viewport();
    const entry: VisibleEntry = {
      target: n,
      isIntersecting: on === 1,
      intersectionRatio: ratio!,
      boundingClientRect: box,
      intersectionRect: on ? box : new BlitzRect(),
      rootBounds: new BlitzRect(0, 0, vp.width, vp.height),
      time: batch.time,
    };
    for (const fn of [...fns]) timed("visible watcher", fn, () => fn(entry));
  }
  const Event = (globalThis as any).Event;
  for (const [id] of batch.scroll ?? []) {
    const n = id === 0 ? (globalThis as any).document : nodes.get(id!);
    n?.dispatchEvent(new Event("scroll", { bubbles: false }));
  }
  const AnimationEvent = (globalThis as any).AnimationEvent;
  const TransitionEvent = (globalThis as any).TransitionEvent;
  for (const [id, type, name, elapsed] of batch.ends ?? []) {
    const n = nodes.get(id);
    if (!n) continue;
    const e = type.startsWith("animation")
      ? new AnimationEvent(type, { bubbles: true, animationName: name, elapsedTime: elapsed })
      : new TransitionEvent(type, { bubbles: true, propertyName: name, elapsedTime: elapsed });
    n.dispatchEvent(e);
  }
  const run = frameCallbacks;
  frameCallbacks = new Map();
  for (const fn of run.values()) timed("frame callback", fn, () => fn(batch.time));
  // The frame's work is done: a task after it, behind every timer already due, is the idle time.
  if (idleCallbacks.size) {
    const waiting = [...idleCallbacks.keys()];
    setTimeout(() => runIdle(waiting), 0);
  }
  frameTimed();
}
