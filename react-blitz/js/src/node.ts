// The DOM subset react-blitz's nodes answer. React's host config and the libraries abyme runs
// (Radix, floating-ui, virtua) call the same members, so each call has one owner: a member writes
// ops through the encoder, or reads the host through `host.ts`.
import { encoder, written } from "./bridge";
import { NS } from "./encoder";
import * as host from "./host";
import * as popover from "./popover";

export const nodes = new Map<number, BlitzNode>();
let nextId = 2;

/** A fresh JS id. Id 1 is a test's container; ids from 2^31 are the host's own nodes. */
export function newId(): number {
  return nextId++;
}

type Listener = { fn: EventListenerOrEventListenerObject; capture: boolean; once: boolean; passive: boolean };
type EventListenerOrEventListenerObject = ((e: any) => void) | { handleEvent(e: any): void };

function captureOf(opts: boolean | AddEventListenerOptions | undefined): boolean {
  return typeof opts === "boolean" ? opts : !!opts?.capture;
}
interface AddEventListenerOptions {
  capture?: boolean;
  once?: boolean;
  passive?: boolean;
  signal?: { aborted: boolean; addEventListener(t: string, fn: () => void): void };
}

export class BlitzEventTarget {
  listeners = new Map<string, Listener[]>();

  /** The node whose LISTEN op stands for this target's listeners. */
  protected listenId(): number | null {
    return null;
  }

  addEventListener(type: string, fn: EventListenerOrEventListenerObject | null, opts?: boolean | AddEventListenerOptions): void {
    if (!fn) return;
    const capture = captureOf(opts);
    const o = typeof opts === "object" ? opts : undefined;
    if (o?.signal?.aborted) return;
    const list = this.listeners.get(type) ?? [];
    if (list.some((l) => l.fn === fn && l.capture === capture)) return;
    list.push({ fn, capture, once: !!o?.once, passive: !!o?.passive });
    this.listeners.set(type, list);
    o?.signal?.addEventListener("abort", () => this.removeEventListener(type, fn, opts));
    const id = this.listenId();
    if (id !== null && list.length === 1) {
      encoder.listen(id, type, true);
      written();
    }
  }

  removeEventListener(type: string, fn: EventListenerOrEventListenerObject | null, opts?: boolean | AddEventListenerOptions): void {
    const list = this.listeners.get(type);
    if (!list || !fn) return;
    const capture = captureOf(opts);
    const at = list.findIndex((l) => l.fn === fn && l.capture === capture);
    if (at < 0) return;
    list.splice(at, 1);
    if (list.length === 0) {
      this.listeners.delete(type);
      const id = this.listenId();
      if (id !== null && !(this instanceof BlitzElement && this.handles(type))) {
        encoder.listen(id, type, false);
        written();
      }
    }
  }

  dispatchEvent(event: any): boolean {
    // Imported late: events.ts imports this file.
    return dispatchHook(this, event);
  }
}

/** Set by events.ts: dispatch an event made in JS through the tree. */
export let dispatchHook: (target: BlitzEventTarget, event: any) => boolean = () => true;
export function setDispatchHook(fn: typeof dispatchHook): void {
  dispatchHook = fn;
}

export abstract class BlitzNode extends BlitzEventTarget {
  static readonly ELEMENT_NODE = 1;
  static readonly TEXT_NODE = 3;
  static readonly COMMENT_NODE = 8;
  static readonly DOCUMENT_NODE = 9;
  static readonly DOCUMENT_FRAGMENT_NODE = 11;
  static readonly DOCUMENT_POSITION_DISCONNECTED = 1;
  static readonly DOCUMENT_POSITION_PRECEDING = 2;
  static readonly DOCUMENT_POSITION_FOLLOWING = 4;
  static readonly DOCUMENT_POSITION_CONTAINS = 8;
  static readonly DOCUMENT_POSITION_CONTAINED_BY = 16;
  readonly ELEMENT_NODE = 1;
  readonly TEXT_NODE = 3;
  readonly DOCUMENT_NODE = 9;

  parentNode: BlitzNode | null = null;
  childNodes: BlitzNode[] = [];
  abstract readonly nodeType: number;
  abstract readonly nodeName: string;

  constructor(readonly rbId: number) {
    super();
    nodes.set(rbId, this);
  }

  protected override listenId(): number | null {
    return this.rbId;
  }

  get ownerDocument(): any {
    return (globalThis as any).document ?? null;
  }
  get parentElement(): BlitzElement | null {
    return this.parentNode instanceof BlitzElement ? this.parentNode : null;
  }
  get firstChild(): BlitzNode | null {
    return this.childNodes[0] ?? null;
  }
  get lastChild(): BlitzNode | null {
    return this.childNodes[this.childNodes.length - 1] ?? null;
  }
  get nextSibling(): BlitzNode | null {
    const p = this.parentNode;
    if (!p) return null;
    return p.childNodes[p.childNodes.indexOf(this) + 1] ?? null;
  }
  get previousSibling(): BlitzNode | null {
    const p = this.parentNode;
    if (!p) return null;
    return p.childNodes[p.childNodes.indexOf(this) - 1] ?? null;
  }
  get isConnected(): boolean {
    let n: BlitzNode | null = this;
    while (n.parentNode) n = n.parentNode;
    return n.nodeType === 9 || (n instanceof BlitzElement && n.adopted);
  }
  hasChildNodes(): boolean {
    return this.childNodes.length > 0;
  }
  getRootNode(): BlitzNode {
    let n: BlitzNode = this;
    while (n.parentNode) n = n.parentNode;
    return n.isConnected ? (globalThis as any).document ?? n : n;
  }
  contains(other: BlitzNode | null): boolean {
    for (let n = other; n; n = n.parentNode) if (n === this) return true;
    return false;
  }
  compareDocumentPosition(other: BlitzNode): number {
    if (other === this) return 0;
    if (this.contains(other)) return 20; // CONTAINED_BY | FOLLOWING
    if (other.contains(this)) return 10; // CONTAINS | PRECEDING
    const path = (n: BlitzNode) => {
      const out: BlitzNode[] = [];
      for (let m: BlitzNode | null = n; m; m = m.parentNode) out.unshift(m);
      return out;
    };
    const a = path(this), b = path(other);
    if (a[0] !== b[0]) return 1;
    let i = 0;
    while (a[i] === b[i]) i++;
    const parent = a[i - 1]!;
    return parent.childNodes.indexOf(b[i]!) > parent.childNodes.indexOf(a[i]!) ? 4 : 2;
  }

  get textContent(): string {
    return this.childNodes.map((c) => c.textContent).join("");
  }
  set textContent(text: string) {
    for (const c of [...this.childNodes]) {
      this.removeChild(c);
      c.free();
    }
    if (text) this.appendChild(new BlitzText(newId(), text));
  }

  appendChild<T extends BlitzNode>(child: T): T {
    if (child.nodeType === 11) {
      for (const c of [...child.childNodes]) this.appendChild(c);
      return child;
    }
    child.detach();
    this.childNodes.push(child);
    child.parentNode = this;
    encoder.append(this.rbId, child.rbId);
    written();
    return child;
  }
  append(...children: (BlitzNode | string)[]): void {
    for (const c of children) this.appendChild(typeof c === "string" ? new BlitzText(newId(), c) : c);
  }
  prepend(...children: (BlitzNode | string)[]): void {
    const first = this.firstChild;
    for (const c of children) {
      const n = typeof c === "string" ? new BlitzText(newId(), c) : c;
      if (first) this.insertBefore(n, first);
      else this.appendChild(n);
    }
  }
  insertBefore<T extends BlitzNode>(child: T, before: BlitzNode | null): T {
    if (!before) return this.appendChild(child);
    if (child.nodeType === 11) {
      for (const c of [...child.childNodes]) this.insertBefore(c, before);
      return child;
    }
    child.detach();
    const at = this.childNodes.indexOf(before);
    this.childNodes.splice(at < 0 ? this.childNodes.length : at, 0, child);
    child.parentNode = this;
    encoder.insertBefore(this.rbId, child.rbId, before.rbId);
    written();
    return child;
  }
  removeChild<T extends BlitzNode>(child: T): T {
    const at = this.childNodes.indexOf(child);
    if (at < 0) return child;
    this.childNodes.splice(at, 1);
    child.parentNode = null;
    encoder.remove(this.rbId, child.rbId);
    written();
    popover.removed(child);
    return child;
  }
  replaceChildren(...children: (BlitzNode | string)[]): void {
    for (const c of [...this.childNodes]) this.removeChild(c);
    this.append(...children);
  }
  remove(): void {
    this.parentNode?.removeChild(this);
  }
  /** ChildNode's: each node or string, in order, just before this one. */
  before(...children: (BlitzNode | string)[]): void {
    const parent = this.parentNode;
    if (!parent) return;
    const placed = children.map((c) => (typeof c === "string" ? new BlitzText(newId(), c) : c));
    let next: BlitzNode | null = this;
    if (placed.includes(this)) next = this.followingNotIn(placed);
    for (const n of placed) parent.insertBefore(n, next);
  }
  /** ChildNode's: each node or string, in order, just after this one. */
  after(...children: (BlitzNode | string)[]): void {
    const parent = this.parentNode;
    if (!parent) return;
    const placed = children.map((c) => (typeof c === "string" ? new BlitzText(newId(), c) : c));
    const next = this.followingNotIn(placed);
    for (const n of placed) if (n !== this) parent.insertBefore(n, next);
  }
  /** ChildNode's: this node out, and each node or string, in order, where it was. */
  replaceWith(...children: (BlitzNode | string)[]): void {
    const parent = this.parentNode;
    if (!parent) return;
    const placed = children.map((c) => (typeof c === "string" ? new BlitzText(newId(), c) : c));
    const next = this.followingNotIn(placed);
    if (!placed.includes(this)) parent.removeChild(this);
    for (const n of placed) parent.insertBefore(n, next);
  }
  /** The DOM's "viable next sibling": the first sibling after this one that is not being placed. */
  private followingNotIn(placed: BlitzNode[]): BlitzNode | null {
    let n = this.nextSibling;
    while (n && placed.includes(n)) n = n.nextSibling;
    return n;
  }
  /** Out of its parent, with no op: an append or insert moves it in one op. */
  protected detach(): void {
    const p = this.parentNode;
    if (!p) return;
    p.childNodes.splice(p.childNodes.indexOf(this), 1);
    this.parentNode = null;
  }
  /** The node and its subtree are gone: the host frees them. */
  free(): void {
    encoder.drop(this.rbId);
    written();
    forget(this);
  }
  cloneNode(deep = false): BlitzNode {
    const copy = this.shallowCopy();
    if (deep) for (const c of this.childNodes) copy.appendChild(c.cloneNode(true));
    return copy;
  }
  protected abstract shallowCopy(): BlitzNode;
}

function forget(n: BlitzNode): void {
  nodes.delete(n.rbId);
  for (const c of n.childNodes) forget(c);
}

export class BlitzText extends BlitzNode {
  readonly nodeType = 3;
  readonly nodeName = "#text";
  private text: string;

  constructor(id: number, text: string, adopted = false) {
    super(id);
    this.text = text;
    if (!adopted) {
      encoder.createText(id, text);
      written();
    }
  }
  get data(): string {
    return this.text;
  }
  set data(text: string) {
    if (text === this.text) return;
    this.text = text;
    encoder.setText(this.rbId, text);
    written();
  }
  get nodeValue(): string {
    return this.text;
  }
  set nodeValue(text: string) {
    this.data = text;
  }
  override get textContent(): string {
    return this.text;
  }
  override set textContent(text: string) {
    this.data = text;
  }
  get length(): number {
    return this.text.length;
  }
  protected shallowCopy(): BlitzNode {
    return new BlitzText(newId(), this.text);
  }
}

const UNITLESS = new Set([
  "animation-iteration-count", "aspect-ratio", "border-image-outset", "border-image-slice", "border-image-width",
  "box-flex", "box-flex-group", "box-ordinal-group", "column-count", "columns", "flex", "flex-grow",
  "flex-positive", "flex-shrink", "flex-negative", "flex-order", "grid-area", "grid-row", "grid-row-end",
  "grid-row-span", "grid-row-start", "grid-column", "grid-column-end", "grid-column-span", "grid-column-start",
  "font-weight", "line-clamp", "line-height", "opacity", "order", "orphans", "scale", "tab-size", "widows",
  "z-index", "zoom", "fill-opacity", "flood-opacity", "stop-opacity", "stroke-dasharray", "stroke-dashoffset",
  "stroke-miterlimit", "stroke-opacity", "stroke-width",
]);

/** `fontSize` to `font-size`; a custom property as it is. */
export function cssName(name: string): string {
  if (name.startsWith("--")) return name;
  const kebab = name.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  return /^(webkit|moz|ms)-/.test(kebab) ? "-" + kebab : kebab;
}

/** React's reading of a style value: a number gets `px` unless the property is unitless. */
export function cssValue(property: string, value: unknown): string | null {
  if (value === null || value === undefined || typeof value === "boolean" || value === "") return null;
  if (typeof value === "number" && value !== 0 && !property.startsWith("--") && !UNITLESS.has(property)) return `${value}px`;
  return String(value).trim();
}

/** An element's inline style: each write is a SET_STYLE or REMOVE_STYLE. */
export class BlitzStyle {
  private values = new Map<string, string>();
  constructor(private readonly el: BlitzElement) {
    return new Proxy(this, {
      get(target, key, receiver) {
        if (typeof key === "string" && !(key in target)) return target.getPropertyValue(cssName(key));
        return Reflect.get(target, key, receiver);
      },
      set(target, key, value) {
        if (typeof key === "string" && !(key in target)) {
          target.setProperty(cssName(key), value);
          return true;
        }
        return Reflect.set(target, key, value);
      },
    });
  }
  setProperty(property: string, value: unknown, priority?: string): void {
    const v = cssValue(property, value);
    if (v === null) return this.removeProperty(property) as unknown as void;
    const full = priority === "important" ? `${v} !important` : v;
    if (this.values.get(property) === full) return;
    this.values.set(property, full);
    encoder.setStyle(this.el.rbId, property, full);
    written();
  }
  removeProperty(property: string): string {
    const old = this.values.get(property) ?? "";
    if (!this.values.delete(property)) return old;
    encoder.removeStyle(this.el.rbId, property);
    written();
    return old;
  }
  getPropertyValue(property: string): string {
    return (this.values.get(property) ?? "").replace(/\s*!important$/, "");
  }
  getPropertyPriority(property: string): string {
    return this.values.get(property)?.endsWith("!important") ? "important" : "";
  }
  get length(): number {
    return this.values.size;
  }
  item(i: number): string {
    return [...this.values.keys()][i] ?? "";
  }
  get cssText(): string {
    return [...this.values].map(([k, v]) => `${k}: ${v};`).join(" ");
  }
  set cssText(text: string) {
    for (const k of [...this.values.keys()]) this.removeProperty(k);
    for (const decl of text.split(";")) {
      const at = decl.indexOf(":");
      if (at > 0) this.setProperty(decl.slice(0, at).trim(), decl.slice(at + 1).trim());
    }
  }
}

class ClassList {
  constructor(private readonly el: BlitzElement) {}
  private get list(): string[] {
    return (this.el.getAttribute("class") ?? "").split(/\s+/).filter(Boolean);
  }
  private set list(names: string[]) {
    this.el.setAttribute("class", names.join(" "));
  }
  contains(name: string): boolean {
    return this.list.includes(name);
  }
  add(...names: string[]): void {
    const l = this.list;
    const add = names.filter((n) => !l.includes(n));
    if (add.length) this.list = [...l, ...add];
  }
  remove(...names: string[]): void {
    const l = this.list;
    if (names.some((n) => l.includes(n))) this.list = l.filter((n) => !names.includes(n));
  }
  toggle(name: string, force?: boolean): boolean {
    const on = force ?? !this.contains(name);
    if (on) this.add(name);
    else this.remove(name);
    return on;
  }
  get length(): number {
    return this.list.length;
  }
  get value(): string {
    return this.el.getAttribute("class") ?? "";
  }
  item(i: number): string | null {
    return this.list[i] ?? null;
  }
  [Symbol.iterator](): Iterator<string> {
    return this.list[Symbol.iterator]();
  }
}

function datasetName(key: string): string {
  return "data-" + key.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
}

/** A DOMRect, as `getBoundingClientRect` answers it. */
export class BlitzRect {
  constructor(public x = 0, public y = 0, public width = 0, public height = 0) {}
  get left(): number {
    return this.x;
  }
  get top(): number {
    return this.y;
  }
  get right(): number {
    return this.x + this.width;
  }
  get bottom(): number {
    return this.y + this.height;
  }
  toJSON() {
    return { x: this.x, y: this.y, width: this.width, height: this.height, top: this.top, left: this.left, right: this.right, bottom: this.bottom };
  }
  static fromRect(r: { x?: number; y?: number; width?: number; height?: number } = {}): BlitzRect {
    return new BlitzRect(r.x, r.y, r.width, r.height);
  }
}

const NS_URI = ["http://www.w3.org/1999/xhtml", "http://www.w3.org/2000/svg", "http://www.w3.org/1998/Math/MathML"];

export class BlitzElement extends BlitzNode {
  readonly nodeType = 1;
  readonly localName: string;
  readonly namespaceURI: string;
  private attrs = new Map<string, string>();
  /** The React props whose `onX` handlers events reach. */
  props: Record<string, any> = {};
  readonly style: BlitzStyle;
  readonly classList: ClassList;
  readonly dataset: Record<string, string>;
  /** A node of the page's own, not made by JS (`body`, `head`). */
  adopted = false;
  /** Form state a field keeps. */
  private fieldValue: string | undefined;
  checked = false;
  disabled = false;

  constructor(id: number, tag: string, ns: number = NS.html, adopted = false) {
    super(id);
    this.localName = tag;
    this.namespaceURI = NS_URI[ns]!;
    this.adopted = adopted;
    this.style = new BlitzStyle(this);
    this.classList = new ClassList(this);
    const el = this;
    this.dataset = new Proxy({} as Record<string, string>, {
      get: (_, k) => (typeof k === "string" ? el.getAttribute(datasetName(k)) ?? undefined : undefined),
      set: (_, k, v) => (typeof k === "string" && el.setAttribute(datasetName(k), String(v)), true),
      deleteProperty: (_, k) => (typeof k === "string" && el.removeAttribute(datasetName(k)), true),
      has: (_, k) => typeof k === "string" && el.hasAttribute(datasetName(k)),
      ownKeys: () => [...el.attrs.keys()].filter((a) => a.startsWith("data-")).map((a) => a.slice(5).replace(/-([a-z])/g, (_m, c) => c.toUpperCase())),
      getOwnPropertyDescriptor: () => ({ enumerable: true, configurable: true }),
    });
    if (!adopted) {
      encoder.createElement(id, tag, ns);
      written();
    }
  }

  get nodeName(): string {
    return this.namespaceURI === NS_URI[0] ? this.localName.toUpperCase() : this.localName;
  }
  get tagName(): string {
    return this.nodeName;
  }

  /** Whether the React props have a handler for the DOM event `type`. */
  handles(type: string): boolean {
    return reactHandles(this.props, type);
  }

  // Attributes
  getAttribute(name: string): string | null {
    if (this.adopted && !this.attrs.has(name)) return host.attr(this, name);
    return this.attrs.get(name) ?? null;
  }
  hasAttribute(name: string): boolean {
    return this.getAttribute(name) !== null;
  }
  setAttribute(name: string, value: unknown): void {
    const v = String(value);
    if (this.attrs.get(name) === v) return;
    this.attrs.set(name, v);
    if (name === "style") {
      this.style.cssText = v;
      return;
    }
    encoder.setAttr(this.rbId, name, v);
    written();
  }
  removeAttribute(name: string): void {
    if (!this.attrs.delete(name)) return;
    if (name === "style") {
      this.style.cssText = "";
      return;
    }
    encoder.removeAttr(this.rbId, name);
    written();
  }
  toggleAttribute(name: string, force?: boolean): boolean {
    const on = force ?? !this.hasAttribute(name);
    if (on) this.setAttribute(name, "");
    else this.removeAttribute(name);
    return on;
  }
  getAttributeNames(): string[] {
    return [...this.attrs.keys()];
  }
  get attributes(): { name: string; value: string }[] {
    return [...this.attrs].map(([name, value]) => ({ name, value }));
  }
  get id(): string {
    return this.getAttribute("id") ?? "";
  }
  set id(v: string) {
    this.setAttribute("id", v);
  }
  get className(): string {
    return this.getAttribute("class") ?? "";
  }
  set className(v: string) {
    this.setAttribute("class", v);
  }
  get tabIndex(): number {
    const t = this.getAttribute("tabindex");
    if (t !== null) return Number(t);
    return ["a", "button", "input", "select", "textarea"].includes(this.localName) ? 0 : -1;
  }
  set tabIndex(v: number) {
    this.setAttribute("tabindex", String(v));
  }
  get hidden(): boolean {
    return this.hasAttribute("hidden");
  }
  set hidden(v: boolean) {
    this.toggleAttribute("hidden", v);
  }
  get inert(): boolean {
    return this.hasAttribute("inert");
  }
  set inert(v: boolean) {
    this.toggleAttribute("inert", v);
  }
  get value(): string {
    return this.fieldValue ?? this.getAttribute("value") ?? "";
  }
  set value(v: string) {
    this.fieldValue = String(v);
    this.setAttribute("value", this.fieldValue);
  }

  // Tree
  get children(): BlitzElement[] {
    return this.childNodes.filter((c): c is BlitzElement => c instanceof BlitzElement);
  }
  get childElementCount(): number {
    return this.children.length;
  }
  get firstElementChild(): BlitzElement | null {
    return this.children[0] ?? null;
  }
  get lastElementChild(): BlitzElement | null {
    const c = this.children;
    return c[c.length - 1] ?? null;
  }
  get nextElementSibling(): BlitzElement | null {
    for (let n = this.nextSibling; n; n = n.nextSibling) if (n instanceof BlitzElement) return n;
    return null;
  }
  get previousElementSibling(): BlitzElement | null {
    for (let n = this.previousSibling; n; n = n.previousSibling) if (n instanceof BlitzElement) return n;
    return null;
  }
  matches(selector: string): boolean {
    return host.matches(this, selector);
  }
  closest(selector: string): BlitzElement | null {
    for (let n: BlitzNode | null = this; n; n = n.parentNode) if (n instanceof BlitzElement && n.matches(selector)) return n;
    return null;
  }
  querySelector(selector: string): BlitzElement | null {
    return host.find(this, selector);
  }
  querySelectorAll(selector: string): BlitzElement[] {
    return host.findAll(this, selector);
  }
  getElementsByTagName(tag: string): BlitzElement[] {
    return this.querySelectorAll(tag);
  }

  // Layout reads
  getBoundingClientRect(): BlitzRect {
    return host.rect(this);
  }
  getClientRects(): BlitzRect[] {
    const r = host.rect(this);
    return r.width || r.height ? [r] : [];
  }
  get offsetWidth(): number {
    return host.metrics(this)[0]!;
  }
  get offsetHeight(): number {
    return host.metrics(this)[1]!;
  }
  get clientWidth(): number {
    return host.metrics(this)[2]!;
  }
  get clientHeight(): number {
    return host.metrics(this)[3]!;
  }
  get scrollWidth(): number {
    return host.metrics(this)[4]!;
  }
  get scrollHeight(): number {
    return host.metrics(this)[5]!;
  }
  get clientLeft(): number {
    return host.metrics(this)[6]!;
  }
  get clientTop(): number {
    return host.metrics(this)[7]!;
  }
  get offsetLeft(): number {
    return host.metrics(this)[8]!;
  }
  get offsetTop(): number {
    return host.metrics(this)[9]!;
  }
  get offsetParent(): BlitzElement | null {
    return host.offsetParent(this);
  }
  get scrollTop(): number {
    return host.scrollPosition(this).y;
  }
  set scrollTop(y: number) {
    host.scrollTo(this, { top: y });
  }
  get scrollLeft(): number {
    return host.scrollPosition(this).x;
  }
  set scrollLeft(x: number) {
    host.scrollTo(this, { left: x });
  }
  scrollTo(a?: number | { top?: number; left?: number; behavior?: string }, b?: number): void {
    host.scrollTo(this, typeof a === "number" ? { left: a, top: b } : a ?? {});
  }
  scroll(a?: number | { top?: number; left?: number }, b?: number): void {
    this.scrollTo(a, b);
  }
  scrollBy(a?: number | { top?: number; left?: number }, b?: number): void {
    const d = typeof a === "number" ? { left: a, top: b ?? 0 } : a ?? {};
    const p = host.scrollPosition(this);
    host.scrollTo(this, { left: p.x + (d.left ?? 0), top: p.y + (d.top ?? 0) });
  }
  scrollIntoView(opts?: boolean | { block?: string; inline?: string }): void {
    host.scrollInto(this, opts);
  }
  checkVisibility(): boolean {
    const r = host.rect(this);
    return r.width > 0 || r.height > 0;
  }
  get innerText(): string {
    return this.textContent;
  }

  // Focus
  focus(_opts?: { preventScroll?: boolean }): void {
    host.focus(this);
  }
  blur(): void {
    host.blur(this);
  }
  click(): void {
    this.dispatchEvent(new (globalThis as any).MouseEvent("click", { bubbles: true, cancelable: true }));
  }

  // Popover: Blitz draws an open one in its top layer and matches it by `:popover-open`.
  get popover(): string | null {
    const state = popover.popoverState(this);
    return state === null ? null : state;
  }
  set popover(v: string | null) {
    if (v === null) this.removeAttribute("popover");
    else this.setAttribute("popover", v);
  }
  showPopover(opts?: { source?: BlitzElement }): void {
    popover.show(this, opts?.source);
  }
  hidePopover(): void {
    popover.hide(this);
  }
  togglePopover(force?: boolean | { force?: boolean; source?: BlitzElement }): boolean {
    return popover.toggle(this, force);
  }

  // Pointer capture: Blitz routes a pressed pointer to its target until release already.
  private captures = new Set<number>();
  setPointerCapture(id: number): void {
    this.captures.add(id);
  }
  releasePointerCapture(id: number): void {
    this.captures.delete(id);
  }
  hasPointerCapture(id: number): boolean {
    return this.captures.has(id);
  }

  getAnimations(): unknown[] {
    return [];
  }

  protected shallowCopy(): BlitzNode {
    const ns = NS_URI.indexOf(this.namespaceURI);
    const copy = new BlitzElement(newId(), this.localName, ns < 0 ? 0 : ns);
    for (const [k, v] of this.attrs) copy.setAttribute(k, v);
    return copy;
  }
}

/** A document fragment: a holder of nodes that an append moves out. */
export class BlitzFragment extends BlitzNode {
  readonly nodeType = 11;
  readonly nodeName = "#document-fragment";
  constructor() {
    super(newId());
    nodes.delete(this.rbId);
  }
  override appendChild<T extends BlitzNode>(child: T): T {
    child.remove();
    this.childNodes.push(child);
    child.parentNode = this;
    return child;
  }
  protected shallowCopy(): BlitzNode {
    return new BlitzFragment();
  }
}

// React's event props: onClick → click, onDoubleClick → dblclick, onFocus → focusin.
const REACT_EVENT: Record<string, string> = {
  doubleclick: "dblclick",
  focus: "focusin",
  blur: "focusout",
  change: "input",
};

/** The DOM event a React prop handles, or null when the prop is not a handler. */
export function eventOfProp(prop: string): { type: string; capture: boolean } | null {
  if (!/^on[A-Z]/.test(prop)) return null;
  let name = prop.slice(2);
  let capture = false;
  if (name.endsWith("Capture")) {
    name = name.slice(0, -7);
    capture = true;
  }
  const lower = name.toLowerCase();
  return { type: REACT_EVENT[lower] ?? lower, capture };
}

function reactHandles(props: Record<string, any>, type: string): boolean {
  for (const k in props) {
    if (typeof props[k] !== "function") continue;
    const e = eventOfProp(k);
    if (e && e.type === type) return true;
  }
  return false;
}
