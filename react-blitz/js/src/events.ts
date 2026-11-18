// Events: the host's input events, one at a time, and events JS dispatches itself, both walked
// through the same path: capture from the root, the target, then bubbling, calling listeners and
// React's `onX` and `onXCapture` props. A handler's `preventDefault` and `stopPropagation` go back
// to the host.
import { currentEvent } from "./config";
import { BlitzElement, BlitzEventTarget, BlitzNode, eventOfProp, setDispatchHook } from "./node";
import * as host from "./host";
import { timed } from "./times";

/** The event kinds the host sends, by index: react-blitz/src/events.rs has the same list. */
export const EVENT_TYPES = [
  "pointermove", "pointerdown", "pointerup", "pointercancel", "pointerenter", "pointerleave",
  "pointerover", "pointerout", "mousemove", "mousedown", "mouseup", "mouseenter", "mouseleave",
  "mouseover", "mouseout", "touchstart", "touchmove", "touchend", "touchcancel", "scroll", "wheel",
  "click", "contextmenu", "dblclick", "keypress", "keydown", "keyup", "input", "focus", "blur",
  "focusin", "focusout",
] as const;

const NON_BUBBLING = new Set(["mouseenter", "mouseleave", "pointerenter", "pointerleave", "focus", "blur", "scroll", "load", "error"]);

/** The window's own listeners: its LISTEN ops stand on the document node. */
export let windowTarget: BlitzEventTarget | null = null;
export function setWindowTarget(t: BlitzEventTarget): void {
  windowTarget = t;
}

function invoke(target: BlitzEventTarget, event: any, capture: boolean, phase: number): void {
  event.currentTarget = target === windowTarget ? globalThis : target;
  event.eventPhase = phase;
  const list = target.listeners.get(event.type);
  if (list) {
    for (const l of [...list]) {
      if (l.capture !== capture) continue;
      if (l.once) target.removeEventListener(event.type, l.fn, { capture: l.capture });
      try {
        const fn = l.fn;
        timed(event.type, fn, () => (typeof fn === "function" ? fn.call(event.currentTarget, event) : fn.handleEvent(event)));
      } catch (e) {
        console.error(e);
      }
      if (event._stopNow) return;
    }
  }
  if (target instanceof BlitzElement) {
    for (const k in target.props) {
      const fn = target.props[k];
      if (typeof fn !== "function") continue;
      const e = eventOfProp(k);
      if (!e || e.type !== event.type) continue;
      if (e.capture !== capture) continue;
      try {
        timed(`${event.type} prop`, fn, () => fn(event));
      } catch (err) {
        console.error(err);
      }
    }
  }
}

/** Walk `path` (target first) as the DOM does. Answers whether the default may run. */
export function propagate(path: BlitzEventTarget[], event: any): boolean {
  if (!path.length) return true;
  event.target = path[0] === windowTarget ? globalThis : path[0];
  event._path = path.map((t) => (t === windowTarget ? globalThis : t));
  // React's SyntheticEvent members, on the event itself.
  event.nativeEvent ??= event;
  event.persist ??= () => {};
  event.isDefaultPrevented ??= () => event.defaultPrevented;
  event.isPropagationStopped ??= () => !!event._stop;
  const bubbles = event.bubbles && !NON_BUBBLING.has(event.type);
  for (let i = path.length - 1; i > 0 && !event._stop; i--) invoke(path[i]!, event, true, 1);
  if (!event._stop) {
    invoke(path[0]!, event, true, 2);
    if (!event._stop) invoke(path[0]!, event, false, 2);
  }
  if (bubbles) for (let i = 1; i < path.length && !event._stop; i++) invoke(path[i]!, event, false, 3);
  event.currentTarget = null;
  event.eventPhase = 0;
  return !event.defaultPrevented;
}

/** The path of a JS node: itself and its ancestors, then the document and window when connected. */
function pathOf(target: BlitzEventTarget): BlitzEventTarget[] {
  if (!(target instanceof BlitzNode)) return (target as unknown) === globalThis ? [windowTarget ?? target] : [target];
  const path: BlitzEventTarget[] = [];
  for (let n: BlitzNode | null = target; n; n = n.parentNode) path.push(n);
  const top = path[path.length - 1] as BlitzNode;
  const doc = (globalThis as any).document as BlitzNode | undefined;
  if (top instanceof BlitzElement && top.adopted && doc && top !== doc) {
    // A node of the page's own: its ancestors are the host's.
    let p = top.parentNode;
    if (!p) {
      const html = doc.childNodes[0];
      if (html && html !== top && top.localName !== "html") path.push(html);
    }
    path.push(doc);
  }
  if (doc && path.includes(doc) && windowTarget) path.push(windowTarget);
  return path;
}

setDispatchHook((target, event) => {
  const was = { ...currentEvent };
  currentEvent.type = event.type;
  try {
    return propagate(pathOf(target), event);
  } finally {
    Object.assign(currentEvent, was);
  }
});

function makeEvent(type: string, fields: Record<string, any>): any {
  const g = globalThis as any;
  const init = { bubbles: !NON_BUBBLING.has(type), cancelable: true, composed: true, ...fields };
  let e: any;
  if (type.startsWith("pointer")) e = new g.PointerEvent(type, init);
  else if (type === "wheel") e = new g.WheelEvent(type, init);
  else if (type.startsWith("mouse") || type === "click" || type === "dblclick" || type === "contextmenu") e = new g.MouseEvent(type, init);
  else if (type.startsWith("key")) e = new g.KeyboardEvent(type, init);
  else if (type.startsWith("touch")) e = new g.TouchEvent(type, init);
  else if (type.startsWith("focus") || type === "blur") e = new g.FocusEvent(type, { ...init, cancelable: false });
  else if (type === "input") e = new g.InputEvent(type, init);
  else e = new g.Event(type, init);
  e.isTrusted = true;
  return e;
}

/** An input event from the host: answers 1 when prevented, 2 when propagation stopped. */
export function dispatchFromHost(kind: number, chain: number[], fields: Record<string, any>): number {
  const type = EVENT_TYPES[kind];
  if (!type) return 0;
  const path: BlitzEventTarget[] = chain.map((id) => host.node(id)).filter((n): n is BlitzNode => n !== null);
  if (windowTarget) path.push(windowTarget);
  const event = makeEvent(type, fields);
  if (event.relatedTarget === undefined) event.relatedTarget = null;
  currentEvent.type = type;
  currentEvent.time = event.timeStamp ?? -1.1;
  try {
    timed(`host ${type}, whole`, null, () => propagate(path, event));
  } finally {
    currentEvent.type = null;
    currentEvent.time = -1.1;
  }
  return (event.defaultPrevented ? 1 : 0) | (event._stop ? 2 : 0);
}
