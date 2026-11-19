// The Popover API, as a browser has it: showPopover, hidePopover and togglePopover, the
// beforetoggle and toggle events, the stack of open auto popovers with its light dismiss (a click
// outside, or Escape). popovertarget buttons are not answered yet. Blitz draws an open popover in its top layer,
// above everything and outside every ancestor's clip, and matches it by `:popover-open`.
// https://html.spec.whatwg.org/multipage/popover.html
import { encoder, written } from "./bridge";
import type { BlitzElement, BlitzNode } from "./node";

type State = "auto" | "manual" | "hint";

/** Every open popover, in the order shown. */
const open: BlitzElement[] = [];
/** The open auto popovers, bottom first: a new one closes those that are not its ancestors. */
const autoStack: BlitzElement[] = [];
/** The element that showed each open popover, when one did (`source`, or a popovertarget button). */
const invokers = new Map<BlitzElement, BlitzElement>();
/** A `toggle` event waiting to fire, per popover: a hide and show in one turn fire once. */
const pendingToggle = new Map<BlitzElement, { oldState: string; newState: string }>();

function domError(name: string, message: string): Error {
  const D = (globalThis as any).DOMException;
  if (D) return new D(message, name);
  const e = new Error(message);
  e.name = name;
  return e;
}

/** The element's popover state, from its `popover` attribute: null when it is not a popover. */
export function popoverState(el: BlitzElement): State | null {
  const v = el.getAttribute("popover");
  if (v === null) return null;
  const k = v.toLowerCase();
  if (k === "" || k === "auto") return "auto";
  if (k === "manual") return "manual";
  if (k === "hint") return "hint";
  // An invalid value is the manual state.
  return "manual";
}

export function isOpen(el: BlitzElement): boolean {
  return open.includes(el);
}

function toggleEvent(type: string, oldState: string, newState: string, cancelable: boolean): any {
  const g = globalThis as any;
  const E = g.ToggleEvent ?? g.Event;
  const e = new E(type, { bubbles: false, cancelable, oldState, newState });
  e.oldState = oldState;
  e.newState = newState;
  return e;
}

function queueToggle(el: BlitzElement, oldState: string, newState: string): void {
  const waiting = pendingToggle.get(el);
  if (waiting) {
    waiting.newState = newState;
    return;
  }
  pendingToggle.set(el, { oldState, newState });
  setTimeout(() => {
    const t = pendingToggle.get(el);
    pendingToggle.delete(el);
    if (t && t.oldState !== t.newState) el.dispatchEvent(toggleEvent("toggle", t.oldState, t.newState, false));
  }, 0);
}

function sendOpen(el: BlitzElement, on: boolean): void {
  encoder.popover(el.rbId, on);
  written();
}

/** The nearest open auto popover holding `node`, by the tree or by an invoker, itself included; an
 * invoker of an open one counts as in it. */
function popoverAncestor(node: BlitzNode | null): BlitzElement | null {
  const seen = new Set<BlitzNode>();
  for (let n: BlitzNode | null = node; n && !seen.has(n); ) {
    seen.add(n);
    const el = n as BlitzElement;
    if (autoStack.includes(el)) return el;
    // An invoker counts as inside the popover it showed, so a click on an open menu's trigger
    // leaves the menu to the trigger's own click.
    for (let i = autoStack.length - 1; i >= 0; i--) {
      if (invokers.get(autoStack[i]!) === el) return autoStack[i]!;
    }
    const invoker = invokers.get(el);
    n = invoker ?? n.parentNode;
  }
  return null;
}

/** Hide every open auto popover above `keep` on the stack (all of them when `keep` is null). */
function hideAllUntil(keep: BlitzElement | null, fireEvents: boolean): void {
  while (autoStack.length && autoStack[autoStack.length - 1] !== keep) {
    hide(autoStack[autoStack.length - 1]!, fireEvents, false);
  }
}

export function show(el: BlitzElement, source?: BlitzElement | null): void {
  const state = popoverState(el);
  if (state === null) throw domError("NotSupportedError", "Not supported on elements that do not have a valid value for the 'popover' attribute.");
  if (isOpen(el)) return;
  if (!el.isConnected) throw domError("InvalidStateError", "Invalid on disconnected popover elements.");
  const go = el.dispatchEvent(toggleEvent("beforetoggle", "closed", "open", true));
  // A listener may have shown it, taken it out, or changed its attribute.
  if (!go || isOpen(el) || !el.isConnected || popoverState(el) !== state) return;
  if (source) invokers.set(el, source);
  if (state === "auto") {
    hideAllUntil(popoverAncestor(source ?? el.parentNode), true);
    if (isOpen(el) || !el.isConnected) return;
    autoStack.push(el);
    listen();
  }
  open.push(el);
  sendOpen(el, true);
  queueToggle(el, "closed", "open");
}

export function hide(el: BlitzElement, fireEvents = true, throwIfClosed = true): void {
  if (!isOpen(el)) {
    if (throwIfClosed && popoverState(el) === null) {
      throw domError("NotSupportedError", "Not supported on elements that do not have a valid value for the 'popover' attribute.");
    }
    return;
  }
  // Auto popovers it holds close first.
  const at = autoStack.indexOf(el);
  if (at >= 0) hideAllUntil(el, fireEvents);
  if (fireEvents) el.dispatchEvent(toggleEvent("beforetoggle", "open", "closed", false));
  if (!isOpen(el)) return;
  open.splice(open.indexOf(el), 1);
  const stacked = autoStack.indexOf(el);
  if (stacked >= 0) autoStack.splice(stacked, 1);
  invokers.delete(el);
  sendOpen(el, false);
  if (fireEvents) queueToggle(el, "open", "closed");
}

export function toggle(el: BlitzElement, force?: boolean | { force?: boolean; source?: BlitzElement }): boolean {
  const opts = typeof force === "object" && force !== null ? force : { force };
  const want = opts.force ?? !isOpen(el);
  if (want) show(el, opts.source);
  else hide(el);
  return isOpen(el);
}

/** `node` left the tree: open popovers in it close, with no events, as a browser's do. */
export function removed(node: BlitzNode): void {
  if (open.length === 0) return;
  for (const el of [...open]) if (node.contains(el) && !el.isConnected) hide(el, false, false);
}

// Light dismiss: a click that starts and ends outside an auto popover closes it and those above
// it; Escape closes the topmost. Listened for on the document once an auto popover opens.
let listening = false;
let downIn: BlitzElement | null = null;

function listen(): void {
  if (listening) return;
  const doc = (globalThis as any).document;
  if (!doc?.addEventListener) return;
  listening = true;
  doc.addEventListener("pointerdown", (e: any) => {
    downIn = popoverAncestor(e.target ?? null);
  }, true);
  doc.addEventListener("pointerup", (e: any) => {
    if (autoStack.length === 0) return;
    const upIn = popoverAncestor(e.target ?? null);
    if (upIn === downIn) hideAllUntil(upIn, true);
    downIn = null;
  }, true);
  doc.addEventListener("keydown", (e: any) => {
    if (e.key !== "Escape" || e.defaultPrevented || autoStack.length === 0) return;
    hide(autoStack[autoStack.length - 1]!);
  });
}
