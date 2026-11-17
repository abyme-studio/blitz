// The window's members, on the global object: viewport, frames, observers, location, storage.
import { read } from "./bridge";
import { setWindowTarget } from "./events";
import * as host from "./host";
import { BlitzElement, BlitzEventTarget, BlitzNode } from "./node";

class WindowTarget extends BlitzEventTarget {
  protected override listenId(): number | null {
    const doc = (globalThis as any).document;
    return doc ? doc.rbId : null;
  }
}

export class ResizeObserver {
  private stops = new Map<BlitzNode, () => void>();
  constructor(private readonly fn: (entries: host.SizeEntry[], o: ResizeObserver) => void) {}
  observe(el: BlitzNode): void {
    if (this.stops.has(el)) return;
    this.stops.set(el, host.observeSize(el, (e) => this.fn([e], this)));
  }
  unobserve(el: BlitzNode): void {
    this.stops.get(el)?.();
    this.stops.delete(el);
  }
  disconnect(): void {
    for (const stop of this.stops.values()) stop();
    this.stops.clear();
  }
}

export class IntersectionObserver {
  private stops = new Map<BlitzNode, () => void>();
  readonly root = null;
  readonly rootMargin = "0px";
  readonly thresholds = [0];
  constructor(private readonly fn: (entries: host.VisibleEntry[], o: IntersectionObserver) => void) {}
  observe(el: BlitzNode): void {
    if (this.stops.has(el)) return;
    this.stops.set(el, host.observeVisible(el, (e) => this.fn([e], this)));
  }
  unobserve(el: BlitzNode): void {
    this.stops.get(el)?.();
    this.stops.delete(el);
  }
  disconnect(): void {
    for (const stop of this.stops.values()) stop();
    this.stops.clear();
  }
  takeRecords(): host.VisibleEntry[] {
    return [];
  }
}

/** A MutationObserver that hears nothing yet: React owns the tree, and Radix's uses are for
 * changes outside React's commits. */
export class MutationObserver {
  constructor(_fn: unknown) {}
  observe(): void {}
  disconnect(): void {}
  takeRecords(): unknown[] {
    return [];
  }
}

class Storage {
  private m = new Map<string, string>();
  get length(): number {
    return this.m.size;
  }
  key(i: number): string | null {
    return [...this.m.keys()][i] ?? null;
  }
  getItem(k: string): string | null {
    return this.m.get(k) ?? null;
  }
  setItem(k: string, v: string): void {
    this.m.set(k, String(v));
  }
  removeItem(k: string): void {
    this.m.delete(k);
  }
  clear(): void {
    this.m.clear();
  }
}

function makeLocation(): any {
  const g = globalThis as any;
  let url = new g.URL(read().address());
  const loc: any = {
    assign(href: string) {
      url = new g.URL(href, url.href);
    },
    replace(href: string) {
      url = new g.URL(href, url.href);
    },
    reload() {},
    toString: () => url.href,
  };
  for (const k of ["href", "protocol", "host", "hostname", "port", "pathname", "search", "hash", "origin"]) {
    Object.defineProperty(loc, k, {
      get: () => url[k],
      set: (v: string) => {
        if (k === "href") url = new g.URL(v, url.href);
        else url[k] = v;
      },
      enumerable: true,
    });
  }
  return loc;
}

/** `matchMedia`, for the queries a page asks of a window: widths and heights, the colour scheme,
 * reduced motion, hover and pointer. */
function matchMedia(query: string): any {
  const vp = host.viewport();
  const test = (q: string): boolean => {
    q = q.trim().toLowerCase();
    if (q === "" || q === "all" || q === "screen") return true;
    if (q.startsWith("not ")) return !test(q.slice(4));
    if (q.includes(" and ")) return q.split(" and ").every(test);
    if (q.includes(",")) return q.split(",").some(test);
    const m = /^\(\s*([a-z-]+)\s*(?::\s*([^)]+))?\)$/.exec(q);
    if (!m) return false;
    const [, feature, raw] = m;
    const value = (raw ?? "").trim();
    const px = () => parseFloat(value) * (value.endsWith("em") || value.endsWith("rem") ? 16 : 1);
    switch (feature) {
      case "min-width":
        return vp.width >= px();
      case "max-width":
        return vp.width <= px();
      case "min-height":
        return vp.height >= px();
      case "max-height":
        return vp.height <= px();
      case "prefers-color-scheme":
        return value === ((globalThis as any).__reactBlitzScheme ?? "light");
      case "prefers-reduced-motion":
        return value === "no-preference";
      case "hover":
        return value === "hover";
      case "any-hover":
        return value === "hover";
      case "pointer":
      case "any-pointer":
        return value === "fine";
      case "orientation":
        return value === (vp.width >= vp.height ? "landscape" : "portrait");
      case "display-mode":
        return value === "browser";
      default:
        return false;
    }
  };
  return {
    media: query,
    matches: test(query),
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: () => true,
  };
}

export function installWindow(): void {
  const g = globalThis as any;
  const target = new WindowTarget();
  setWindowTarget(target);
  g.window = g;
  g.addEventListener = target.addEventListener.bind(target);
  g.removeEventListener = target.removeEventListener.bind(target);
  g.dispatchEvent = (e: any) => target.dispatchEvent(e);
  g.ResizeObserver = ResizeObserver;
  g.IntersectionObserver = IntersectionObserver;
  g.MutationObserver = MutationObserver;
  g.getComputedStyle = (el: BlitzNode) => host.computedStyle(el);
  g.requestAnimationFrame = (fn: (t: number) => void) => host.nextFrame(fn);
  g.cancelAnimationFrame = (h: number) => host.cancelFrame(h);
  g.requestIdleCallback = (fn: (d: any) => void) => g.setTimeout(() => fn({ didTimeout: false, timeRemaining: () => 8 }), 1);
  g.cancelIdleCallback = (h: number) => g.clearTimeout(h);
  g.matchMedia = matchMedia;
  g.localStorage = new Storage();
  g.sessionStorage = new Storage();
  g.location = makeLocation();
  const entries: any[] = [{ state: null }];
  g.history = {
    get length() {
      return entries.length;
    },
    get state() {
      return entries[entries.length - 1].state;
    },
    pushState(state: unknown, _t: string, url?: string) {
      entries.push({ state });
      if (url) g.location.assign(url);
    },
    replaceState(state: unknown, _t: string, url?: string) {
      entries[entries.length - 1] = { state };
      if (url) g.location.replace(url);
    },
    back() {},
    forward() {},
    go() {},
    scrollRestoration: "auto",
  };
  g.navigator = {
    userAgent: "Mozilla/5.0 (react-blitz)",
    platform: "MacIntel",
    language: "en-US",
    languages: ["en-US", "en"],
    maxTouchPoints: 0,
    onLine: true,
    hardwareConcurrency: 4,
    vendor: "",
    cookieEnabled: false,
    clipboard: { writeText: () => Promise.resolve(), readText: () => Promise.resolve(""), write: () => Promise.resolve(), read: () => Promise.resolve([]) },
    userAgentData: undefined,
  };
  for (const [k, get] of Object.entries({
    innerWidth: () => host.viewport().width,
    innerHeight: () => host.viewport().height,
    outerWidth: () => host.viewport().width,
    outerHeight: () => host.viewport().height,
    devicePixelRatio: () => host.viewport().scale,
    scrollX: () => host.scrollPosition(g.document.documentElement).x,
    scrollY: () => host.scrollPosition(g.document.documentElement).y,
    pageXOffset: () => host.scrollPosition(g.document.documentElement).x,
    pageYOffset: () => host.scrollPosition(g.document.documentElement).y,
  })) {
    Object.defineProperty(g, k, { get, configurable: true });
  }
  g.screen = { get width() { return host.viewport().width; }, get height() { return host.viewport().height; }, availWidth: 0, availHeight: 0, colorDepth: 24, orientation: { type: "landscape-primary", angle: 0, addEventListener() {} } };
  g.scrollTo = (a: any, b?: number) => g.document.documentElement.scrollTo(a, b);
  g.scroll = g.scrollTo;
  g.scrollBy = (a: any, b?: number) => g.document.documentElement.scrollBy(a, b);
  g.focus = () => {};
  g.blur = () => {};
  g.parent = g;
  g.top = g;
  g.opener = null;
  g.frameElement = null;
  g.closed = false;
  g.name = "";
  g.isSecureContext = true;
  g.origin = g.location.origin;
  g.postMessage = (data: unknown) => {
    g.setTimeout(() => target.dispatchEvent(Object.assign(new g.Event("message"), { data, source: g, origin: g.location.origin })), 0);
  };
  g.getSelection = () => ({
    rangeCount: 0,
    isCollapsed: true,
    type: "None",
    anchorNode: null,
    focusNode: null,
    removeAllRanges() {},
    addRange() {},
    collapse() {},
    getRangeAt() {
      return g.document.createRange();
    },
    toString: () => "",
  });
  g.CSS = {
    supports: (a: string, b?: string) => (b === undefined ? true : true),
    escape: (s: string) => String(s).replace(/([^\w-])/g, "\\$1"),
  };
  void BlitzElement;
}
