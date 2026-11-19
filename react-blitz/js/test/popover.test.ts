// The Popover API and ChildNode's before, after and replaceWith on react-blitz's nodes: what UI
// SDK's menu reaches for. Blitz's half (the top layer, :popover-open) is the fork's
// react-blitz-shot test `tests/popover.rs`.
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { encoder, type Bridge } from "../src/bridge";
import "../src/events";
import { BlitzElement, BlitzText, nodes } from "../src/node";

// The page's events as react-blitz's V8 has them: plain objects whose target dispatch sets
// (Node's own Event has a getter-only target).
class PageEvent {
  defaultPrevented = false;
  bubbles: boolean;
  cancelable: boolean;
  constructor(readonly type: string, init: Record<string, any> = {}) {
    this.bubbles = !!init.bubbles;
    this.cancelable = !!init.cancelable;
    Object.assign(this, init);
  }
  preventDefault() {
    if (this.cancelable) this.defaultPrevented = true;
  }
  stopPropagation() {}
}
(globalThis as any).ToggleEvent = PageEvent;

let body: BlitzElement;
let shown: [number, boolean][] = [];

beforeEach(() => {
  globalThis.__reactBlitz = { commit: () => {} } as unknown as Bridge;
  nodes.delete(1);
  body = new BlitzElement(1, "body", 0, true);
  shown = [];
  vi.spyOn(encoder, "popover").mockImplementation((id: number, open: boolean) => {
    shown.push([id, open]);
  });
});

afterEach(() => {
  for (const el of [...body.childNodes]) el.remove();
  vi.restoreAllMocks();
});

const tick = () => new Promise((r) => setTimeout(r, 0));
const el = (popover?: string) => {
  const e = new BlitzElement(Math.floor(Math.random() * 1e9), "div");
  if (popover !== undefined) e.setAttribute("popover", popover);
  return e;
};
const names = (n: BlitzElement) => n.childNodes.map((c) => (c instanceof BlitzText ? c.textContent : (c as BlitzElement).id));

test("after, before and replaceWith place nodes and strings beside this one, in order", () => {
  const [a, b, c, d] = ["a", "b", "c", "d"].map((id) => {
    const e = el();
    e.id = id;
    return e;
  });
  body.append(a!, b!);
  a!.after(c!, "t");
  expect(names(body)).toEqual(["a", "c", "t", "b"]);
  b!.before(d!);
  expect(names(body)).toEqual(["a", "c", "t", "d", "b"]);
  c!.replaceWith(b!);
  expect(names(body)).toEqual(["a", "b", "t", "d"]);
  b!.after(b!, a!);
  expect(names(body)).toEqual(["b", "a", "t", "d"]);
});

test("showPopover opens it in Blitz, after a cancelable beforetoggle, then fires toggle once", async () => {
  const p = el("manual");
  body.append(p);
  const events: string[] = [];
  p.addEventListener("beforetoggle", (e: any) => events.push(`before ${e.oldState}>${e.newState} ${e.cancelable}`));
  p.addEventListener("toggle", (e: any) => events.push(`toggle ${e.oldState}>${e.newState}`));
  p.showPopover();
  p.showPopover();
  expect(shown).toEqual([[p.rbId, true]]);
  expect(events).toEqual(["before closed>open true"]);
  await tick();
  expect(events).toEqual(["before closed>open true", "toggle closed>open"]);
  p.hidePopover();
  expect(shown).toEqual([[p.rbId, true], [p.rbId, false]]);
  await tick();
  expect(events.slice(2)).toEqual(["before open>closed false", "toggle open>closed"]);
});

test("a canceled beforetoggle keeps it closed", () => {
  const p = el("auto");
  body.append(p);
  p.addEventListener("beforetoggle", (e: any) => e.preventDefault());
  p.showPopover();
  expect(shown).toEqual([]);
  expect(p.togglePopover()).toBe(false);
});

test("showPopover throws on a non-popover or a disconnected one", () => {
  const plain = el();
  body.append(plain);
  expect(() => plain.showPopover()).toThrow(/popover/);
  expect(() => el("auto").showPopover()).toThrow(/disconnected/);
});

test("a new auto popover closes the open ones that do not hold it; a nested one stays", () => {
  const menu = el("auto");
  const sub = el("auto");
  const other = el("auto");
  menu.append(sub);
  body.append(menu, other);
  menu.showPopover();
  sub.showPopover();
  expect(shown).toEqual([[menu.rbId, true], [sub.rbId, true]]);
  other.showPopover();
  expect(shown.slice(2)).toEqual([[sub.rbId, false], [menu.rbId, false], [other.rbId, true]]);
  expect(menu.togglePopover(true)).toBe(true);
  expect(other.togglePopover()).toBe(true);
  expect(menu.togglePopover(false)).toBe(false);
});

test("an open popover taken out of the tree closes, with no events", async () => {
  const p = el("manual");
  body.append(p);
  p.showPopover();
  const events: string[] = [];
  p.addEventListener("beforetoggle", () => events.push("before"));
  p.remove();
  expect(shown).toEqual([[p.rbId, true], [p.rbId, false]]);
  await tick();
  expect(events).toEqual([]);
});

test("a click on the trigger that showed an open menu does not light-dismiss it; a click outside does", () => {
  const handlers: Record<string, (e: any) => void> = {};
  const g = globalThis as any;
  const had = g.document;
  // A stand-in document: an element that keeps the listeners light dismiss adds.
  const doc = el();
  vi.spyOn(doc, "addEventListener").mockImplementation(((type: string, fn: (e: any) => void) => (handlers[type] = fn)) as any);
  g.document = doc;
  try {
    const trigger = el();
    const outside = el();
    const menu = el("auto");
    body.append(trigger, outside, menu);
    menu.showPopover({ source: trigger });
    const click = (target: BlitzElement) => {
      handlers.pointerdown!({ target });
      handlers.pointerup!({ target });
    };
    click(trigger);
    expect(shown).toEqual([[menu.rbId, true]]);
    click(outside);
    expect(shown).toEqual([[menu.rbId, true], [menu.rbId, false]]);
  } finally {
    g.document = had;
  }
});
