// The React host config: react-reconciler in mutation mode over react-blitz's nodes. Each mutation
// is a node member that writes ops; `resetAfterCommit` sends the commit as one call.
import { createContext } from "react";
import { beginCommit, endCommit } from "./bridge";
import { NS } from "./encoder";
import * as host from "./host";
import { BlitzElement, BlitzNode, BlitzText, newId } from "./node";
import { applyProps, initialProps } from "./props";

// react-reconciler/constants, by value, so nothing reads `process` at run time.
export const NoEventPriority = 0;
export const DiscreteEventPriority = 2;
export const ContinuousEventPriority = 8;
export const DefaultEventPriority = 32;

type HostContext = number; // the namespace children are made in

let currentPriority = NoEventPriority;
/** The type of the event being dispatched, set by events.ts. */
export const currentEvent: { type: string | null; time: number } = { type: null, time: -1.1 };

const CONTINUOUS = new Set(["drag", "dragenter", "dragexit", "dragleave", "dragover", "mousemove", "mouseout", "mouseover", "pointermove", "pointerout", "pointerover", "scroll", "touchmove", "wheel", "mouseenter", "mouseleave", "pointerenter", "pointerleave"]);

export function priorityOf(type: string): number {
  return CONTINUOUS.has(type) ? ContinuousEventPriority : DiscreteEventPriority;
}

function childNamespace(parent: HostContext, type: string): HostContext {
  if (type === "svg") return NS.svg;
  if (type === "math") return NS.mathml;
  if (parent === NS.svg && type === "foreignObject") return NS.html;
  return parent;
}

export const hostConfig: any = {
  rendererPackageName: "react-blitz",
  rendererVersion: "0.1.0",
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  supportsMicrotasks: true,
  supportsResources: false,
  supportsSingletons: false,
  supportsTestSelectors: false,
  isPrimaryRenderer: true,
  warnsIfNotActing: false,
  noTimeout: -1,
  NotPendingTransition: null,
  HostTransitionContext: createContext(null),

  createInstance(type: string, props: Record<string, any>, _root: unknown, ctx: HostContext): BlitzElement {
    const el = new BlitzElement(newId(), type, childNamespace(ctx, type));
    initialProps(el, props);
    return el;
  },
  createTextInstance(text: string): BlitzText {
    return new BlitzText(newId(), text);
  },
  appendInitialChild(parent: BlitzNode, child: BlitzNode): void {
    parent.appendChild(child);
  },
  finalizeInitialChildren(_el: BlitzElement, _type: string, props: Record<string, any>): boolean {
    return !!props.autoFocus;
  },
  shouldSetTextContent(): boolean {
    return false;
  },
  getRootHostContext(): HostContext {
    return NS.html;
  },
  getChildHostContext(parent: HostContext, type: string): HostContext {
    return childNamespace(parent, type);
  },
  getPublicInstance(instance: BlitzNode): BlitzNode {
    return instance;
  },
  prepareForCommit(): null {
    beginCommit();
    return null;
  },
  resetAfterCommit(): void {
    endCommit();
  },
  preparePortalMount(): void {},
  scheduleTimeout: (fn: () => void, delay?: number) => setTimeout(fn, delay),
  cancelTimeout: (id: any) => clearTimeout(id),
  scheduleMicrotask: (fn: () => void) => queueMicrotask(fn),

  setCurrentUpdatePriority(p: number): void {
    currentPriority = p;
  },
  getCurrentUpdatePriority(): number {
    return currentPriority;
  },
  resolveUpdatePriority(): number {
    if (currentPriority !== NoEventPriority) return currentPriority;
    return currentEvent.type ? priorityOf(currentEvent.type) : DefaultEventPriority;
  },
  resolveEventType(): string | null {
    return currentEvent.type;
  },
  resolveEventTimeStamp(): number {
    return currentEvent.time;
  },
  shouldAttemptEagerTransition(): boolean {
    return false;
  },
  trackSchedulerEvent(): void {},
  requestPostPaintCallback(fn: (t: number) => void): void {
    host.nextFrame((t) => host.nextFrame(() => fn(t)));
  },
  getInstanceFromNode(): null {
    return null;
  },
  beforeActiveInstanceBlur(): void {},
  afterActiveInstanceBlur(): void {},
  prepareScopeUpdate(): void {},
  getInstanceFromScope(): null {
    return null;
  },
  detachDeletedInstance(node: BlitzNode): void {
    node.free();
  },
  bindToConsole(method: string, args: unknown[], badge: string) {
    return Function.prototype.bind.call((console as any)[method], console, `[${badge}]`, ...args);
  },

  // Mutation
  appendChild(parent: BlitzNode, child: BlitzNode): void {
    parent.appendChild(child);
  },
  appendChildToContainer(container: BlitzNode, child: BlitzNode): void {
    container.appendChild(child);
  },
  insertBefore(parent: BlitzNode, child: BlitzNode, before: BlitzNode): void {
    parent.insertBefore(child, before);
  },
  insertInContainerBefore(container: BlitzNode, child: BlitzNode, before: BlitzNode): void {
    container.insertBefore(child, before);
  },
  removeChild(parent: BlitzNode, child: BlitzNode): void {
    parent.removeChild(child);
    // React detaches only elements; a text node is gone once removed.
    if (child instanceof BlitzText) child.free();
  },
  removeChildFromContainer(container: BlitzNode, child: BlitzNode): void {
    container.removeChild(child);
    if (child instanceof BlitzText) child.free();
  },
  resetTextContent(el: BlitzElement): void {
    el.textContent = "";
  },
  commitTextUpdate(text: BlitzText, _old: string, next: string): void {
    text.data = next;
  },
  commitMount(el: BlitzElement, _type: string, props: Record<string, any>): void {
    if (props.autoFocus) el.focus();
  },
  commitUpdate(el: BlitzElement, _type: string, prev: Record<string, any>, next: Record<string, any>): void {
    applyProps(el, prev, next);
  },
  hideInstance(el: BlitzElement): void {
    el.style.setProperty("display", "none", "important");
  },
  hideTextInstance(text: BlitzText): void {
    text.data = "";
  },
  unhideInstance(el: BlitzElement, props: Record<string, any>): void {
    el.style.removeProperty("display");
    const d = props.style?.display;
    if (d !== undefined && d !== null) el.style.setProperty("display", d);
  },
  unhideTextInstance(text: BlitzText, value: string): void {
    text.data = value;
  },
  clearContainer(container: BlitzNode): void {
    for (const c of [...container.childNodes]) container.removeChild(c);
  },

  // Suspensey commits: none; nothing waits.
  maySuspendCommit: () => false,
  maySuspendCommitOnUpdate: () => false,
  maySuspendCommitInSyncRender: () => false,
  preloadInstance: () => true,
  startSuspendingCommit: () => null,
  suspendInstance(): void {},
  suspendOnActiveViewTransition(): void {},
  waitForCommitToBeReady: () => null,
  getSuspendedCommitReason: () => null,
  mayResourceSuspendCommit: () => false,
  preloadResource: () => true,
  suspendResource(): void {},

  resetFormInstance(): void {},

  // View transitions: none; the commit runs straight through.
  startViewTransition(_s: unknown, _c: unknown, _t: unknown, mutation: () => void, layout: () => void, afterMutation: () => void, spawned: () => void, passive: () => void): null {
    mutation();
    layout();
    afterMutation();
    spawned();
    passive();
    return null;
  },
  startGestureTransition: () => null,
  stopViewTransition(): void {},
  addViewTransitionFinishedListener(): void {},
  createViewTransitionInstance: () => null,
  applyViewTransitionName(): void {},
  restoreViewTransitionName(): void {},
  cancelViewTransitionName(): void {},
  cancelRootViewTransitionName(): void {},
  restoreRootViewTransitionName(): void {},
  cloneRootViewTransitionContainer(): void {},
  removeRootViewTransitionClone(): void {},
  measureInstance: () => null,
  measureClonedInstance: () => null,
  wasInstanceInViewport: () => true,
  hasInstanceChanged: () => false,
  hasInstanceAffectedParent: () => false,
  getCurrentGestureOffset: () => 0,

  // Fragment refs
  createFragmentInstance: () => null,
  updateFragmentInstanceFiber(): void {},
  commitNewChildToFragmentInstance(): void {},
  deleteChildFromFragmentInstance(): void {},

  // Hoistables and singletons: none.
  isHostHoistableType: () => false,
  isHostSingletonType: () => false,
  isSingletonScope: () => false,
  getHoistableRoot: () => null,
  prepareToCommitHoistables(): void {},
  setFocusIfFocusable(node: BlitzNode): boolean {
    if (!(node instanceof BlitzElement)) return false;
    node.focus();
    return host.focused() === node;
  },
  setupIntersectionObserver: () => ({ disconnect() {}, observe() {}, unobserve() {} }),
  matchAccessibilityRole: () => false,
  getTextContent: (n: BlitzNode) => n.textContent,
  isHiddenSubtree: () => false,
  extraDevToolsConfig: null,
};
