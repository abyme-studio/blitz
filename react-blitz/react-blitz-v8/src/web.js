// The web globals a page needs that deno_core does not give, and the bridge to the host.
// deno_core gives console, queueMicrotask, structuredClone (Deno.core) and timers (Deno.core).
"use strict";
((g) => {
  const core = g.Deno.core;
  const ops = core.ops;

  // The bridge react-blitz/js calls (its src/bridge.ts).
  g.__reactBlitz = {
    commit: (o, s) => ops.op_rb_commit(new Uint8Array(o.buffer, o.byteOffset, o.byteLength), s),
    rect: (id) => ops.op_rb_rect(id),
    metrics: (id) => ops.op_rb_metrics(id),
    offsetParent: (id) => ops.op_rb_offset_parent(id),
    scroll: (id) => ops.op_rb_scroll(id),
    scrollTo: (id, x, y) => ops.op_rb_scroll_to(id, x, y),
    style: (id, p) => ops.op_rb_style(id, p),
    focus: (id) => ops.op_rb_focus(id),
    blur: (id) => ops.op_rb_blur(id),
    focused: () => ops.op_rb_focused(),
    matches: (id, s) => ops.op_rb_matches(id, s),
    find: (id, s) => ops.op_rb_find(id, s),
    findAll: (id, s) => ops.op_rb_find_all(id, s),
    observe: (id, kind, on) => ops.op_rb_observe(id, kind, on),
    frame: () => ops.op_rb_frame(),
    viewport: () => ops.op_rb_viewport(),
    textWidth: (text, family, size, weight, italic) => ops.op_rb_text_width(text, family, size, weight, italic),
    adopt: (w) => ops.op_rb_adopt(w),
    at: (x, y) => ops.op_rb_at(x, y),
    tag: (id) => ops.op_rb_tag(id),
    attr: (id, n) => ops.op_rb_attr(id, n)[0] ?? null,
    children: (id) => ops.op_rb_children(id),
    hasFocus: () => ops.op_rb_has_focus(),
    address: () => ops.op_rb_address(),
    log: (level, text) => ops.op_rb_log(level, String(text)),
  };

  // What the host calls: an input event, and the frame batch. react-blitz/js sets the handlers.
  g.__reactBlitzEngine = {
    dispatch() {
      const e = ops.op_rb_event_in();
      const h = g.__reactBlitzHandlers;
      let flags = 0;
      if (e && h) {
        try {
          flags = h.dispatch(e.kind, e.chain, JSON.parse(e.fields)) | 0;
        } catch (err) {
          console.error(err);
        }
      }
      ops.op_rb_event_out(flags);
    },
    frame() {
      const b = ops.op_rb_frame_in();
      const h = g.__reactBlitzHandlers;
      if (!b || !h) return;
      try {
        h.frame(JSON.parse(b));
      } catch (err) {
        console.error(err);
      }
    },
  };

  // console goes to the host's log.
  const fmt = (args) =>
    args
      .map((a) => {
        if (typeof a === "string") return a;
        if (a instanceof Error) return `${a.name}: ${a.message}\n${a.stack ?? ""}`;
        try {
          return JSON.stringify(a);
        } catch {
          return String(a);
        }
      })
      .join(" ");
  const logger = (level) => (...args) => ops.op_rb_log(level, fmt(args));
  g.console = {
    log: logger("log"),
    info: logger("info"),
    debug: logger("debug"),
    warn: logger("warn"),
    error: logger("error"),
    trace: logger("trace"),
    group: logger("log"),
    groupCollapsed: logger("log"),
    groupEnd() {},
    time() {},
    timeEnd() {},
    table: logger("log"),
    assert: (c, ...a) => c || ops.op_rb_log("error", fmt(["Assertion failed:", ...a])),
    dir: logger("log"),
    count() {},
  };

  // Timers. On a stepped clock they fall due on that clock: the engine's pump runs one due timer
  // at a time (`__reactBlitzTimers`), earliest first, then its microtasks, as a browser's loop does.
  const timers = new Map();
  const stepped = new Map();
  let nextTimer = 1;
  let nesting = 0;
  function makeTimer(fn, ms, args, repeat) {
    const id = nextTimer++;
    const delay = Math.max(0, Number(ms) || 0);
    if (ops.op_rb_stepped()) {
      const cb = typeof fn === "function" ? () => fn(...args) : () => {};
      // HTML's clamp: a timer nested more than 5 deep waits at least 4 ms, so a chain of 0 ms
      // timeouts moves with the clock instead of spinning the frame.
      const level = nesting + 1;
      const wait = level > 5 ? Math.max(delay, 4) : delay;
      stepped.set(id, { at: ops.op_rb_now() + wait, delay: wait, cb, repeat, level });
      return id;
    }
    const cb = typeof fn === "function" ? () => {
      if (!repeat) timers.delete(id);
      fn(...args);
    } : () => {};
    timers.set(id, core.createTimer(cb, ms, undefined, repeat, true));
    return id;
  }
  function clear(id) {
    stepped.delete(id);
    const t = timers.get(id);
    if (t) {
      core.cancelTimer(t);
      timers.delete(id);
    }
  }
  g.__reactBlitzTimers = () => {
    const now = ops.op_rb_now();
    let next;
    for (const [id, t] of stepped) {
      if (t.at <= now && (!next || t.at < next[1].at)) next = [id, t];
    }
    if (!next) return;
    const [id, t] = next;
    // A browser clamps a repeating timer to 4 ms, so one of 0 ms cannot spin a frame forever.
    if (t.repeat) t.at += Math.max(t.delay, 4);
    else stepped.delete(id);
    ops.op_rb_timer_ran();
    nesting = t.level;
    try {
      t.cb();
    } catch (err) {
      console.error(err);
    } finally {
      nesting = 0;
    }
  };
  g.setTimeout = (fn, ms = 0, ...args) => makeTimer(fn, ms, args, false);
  g.setInterval = (fn, ms = 0, ...args) => makeTimer(fn, ms, args, true);
  g.clearTimeout = clear;
  g.clearInterval = clear;
  g.setImmediate = (fn, ...args) => makeTimer(fn, 0, args, false);
  g.clearImmediate = clear;

  // performance
  const marks = new Map();
  g.performance = {
    now: () => ops.op_rb_now(),
    timeOrigin: Date.now(),
    mark(name) {
      marks.set(name, ops.op_rb_now());
    },
    measure() {},
    getEntriesByName: () => [],
    getEntriesByType: () => [],
    clearMarks() {},
    clearMeasures() {},
  };

  // Events
  class Event {
    constructor(type, init = {}) {
      this.type = String(type);
      this.bubbles = !!init.bubbles;
      this.cancelable = !!init.cancelable;
      this.composed = !!init.composed;
      this.defaultPrevented = false;
      this.timeStamp = ops.op_rb_now();
      this.isTrusted = false;
      this.target = null;
      this.currentTarget = null;
      this.eventPhase = 0;
      this._stop = false;
      this._stopNow = false;
    }
    preventDefault() {
      if (this.cancelable) this.defaultPrevented = true;
    }
    stopPropagation() {
      this._stop = true;
    }
    stopImmediatePropagation() {
      this._stop = true;
      this._stopNow = true;
    }
    get cancelBubble() {
      return this._stop;
    }
    set cancelBubble(v) {
      if (v) this._stop = true;
    }
    composedPath() {
      return this._path ?? [];
    }
    get returnValue() {
      return !this.defaultPrevented;
    }
  }
  Event.NONE = 0;
  Event.CAPTURING_PHASE = 1;
  Event.AT_TARGET = 2;
  Event.BUBBLING_PHASE = 3;
  g.Event = Event;
  g.CustomEvent = class CustomEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.detail = init.detail ?? null;
    }
  };

  class EventTarget {
    #l = new Map();
    addEventListener(type, fn, opts) {
      if (!fn) return;
      const capture = typeof opts === "boolean" ? opts : !!opts?.capture;
      const once = typeof opts === "object" && !!opts?.once;
      const list = this.#l.get(type) ?? [];
      if (list.some((l) => l.fn === fn && l.capture === capture)) return;
      list.push({ fn, capture, once });
      this.#l.set(type, list);
      opts?.signal?.addEventListener?.("abort", () => this.removeEventListener(type, fn, opts));
    }
    removeEventListener(type, fn, opts) {
      const capture = typeof opts === "boolean" ? opts : !!opts?.capture;
      const list = this.#l.get(type);
      if (!list) return;
      const at = list.findIndex((l) => l.fn === fn && l.capture === capture);
      if (at >= 0) list.splice(at, 1);
    }
    dispatchEvent(event) {
      try {
        event.target ??= this;
      } catch {}
      event.currentTarget = this;
      for (const l of [...(this.#l.get(event.type) ?? [])]) {
        if (l.once) this.removeEventListener(event.type, l.fn, l.capture);
        try {
          typeof l.fn === "function" ? l.fn.call(this, event) : l.fn.handleEvent(event);
        } catch (err) {
          console.error(err);
        }
        if (event._stopNow) break;
      }
      const on = this["on" + event.type];
      if (typeof on === "function") on.call(this, event);
      return !event.defaultPrevented;
    }
  }
  g.EventTarget = EventTarget;

  class AbortSignal extends EventTarget {
    constructor() {
      super();
      this.aborted = false;
      this.reason = undefined;
      this.onabort = null;
    }
    throwIfAborted() {
      if (this.aborted) throw this.reason;
    }
    static abort(reason) {
      const c = new AbortController();
      c.abort(reason);
      return c.signal;
    }
    static timeout(ms) {
      const c = new AbortController();
      g.setTimeout(() => c.abort(new DOMException("timed out", "TimeoutError")), ms);
      return c.signal;
    }
    static any(signals) {
      const c = new AbortController();
      for (const s of signals) {
        if (s.aborted) c.abort(s.reason);
        else s.addEventListener("abort", () => c.abort(s.reason));
      }
      return c.signal;
    }
  }
  class AbortController {
    constructor() {
      this.signal = new AbortSignal();
    }
    abort(reason) {
      const s = this.signal;
      if (s.aborted) return;
      s.aborted = true;
      s.reason = reason ?? new DOMException("aborted", "AbortError");
      s.dispatchEvent(new Event("abort"));
    }
  }
  g.AbortSignal = AbortSignal;
  g.AbortController = AbortController;

  class DOMException extends Error {
    constructor(message = "", name = "Error") {
      super(message);
      this.name = name;
    }
  }
  g.DOMException = DOMException;

  // MessageChannel: each message is its own task, as in a browser.
  class MessagePort extends EventTarget {
    constructor() {
      super();
      this.onmessage = null;
      this._other = null;
      this._closed = false;
    }
    postMessage(data) {
      const other = this._other;
      if (!other || this._closed) return;
      const copy = core.structuredClone ? core.structuredClone(data) : data;
      g.setTimeout(() => {
        if (!other._closed) other.dispatchEvent(Object.assign(new Event("message"), { data: copy }));
      }, 0);
    }
    start() {}
    close() {
      this._closed = true;
    }
  }
  g.MessagePort = MessagePort;
  g.MessageChannel = class MessageChannel {
    constructor() {
      this.port1 = new MessagePort();
      this.port2 = new MessagePort();
      this.port1._other = this.port2;
      this.port2._other = this.port1;
    }
  };
  g.MessageEvent = class MessageEvent extends Event {
    constructor(type, init = {}) {
      super(type, init);
      this.data = init.data;
    }
  };

  if (typeof g.structuredClone !== "function") {
    g.structuredClone = (v) => core.structuredClone(v);
  }

  // Text
  g.TextEncoder = class TextEncoder {
    get encoding() {
      return "utf-8";
    }
    encode(s = "") {
      return core.encode(String(s));
    }
    encodeInto(s, dest) {
      const b = core.encode(String(s));
      const n = Math.min(b.length, dest.length);
      dest.set(b.subarray(0, n));
      return { read: s.length, written: n };
    }
  };
  g.TextDecoder = class TextDecoder {
    constructor(label = "utf-8") {
      this.encoding = label;
    }
    decode(buf) {
      if (!buf) return "";
      const u8 = buf instanceof Uint8Array ? buf : ArrayBuffer.isView(buf) ? new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength) : new Uint8Array(buf);
      return core.decode(u8);
    }
  };

  // URL
  class URLSearchParams {
    #p = [];
    #url = null;
    constructor(init = "") {
      if (typeof init === "string") {
        for (const part of init.replace(/^\?/, "").split("&")) {
          if (!part) continue;
          const at = part.indexOf("=");
          const dec = (s) => decodeURIComponent(s.replace(/\+/g, " "));
          this.#p.push(at < 0 ? [dec(part), ""] : [dec(part.slice(0, at)), dec(part.slice(at + 1))]);
        }
      } else if (init && typeof init === "object") {
        const entries = Symbol.iterator in init ? init : Object.entries(init);
        for (const [k, v] of entries) this.#p.push([String(k), String(v)]);
      }
    }
    _bind(url) {
      this.#url = url;
    }
    #changed() {
      if (this.#url) this.#url._setSearch(this.toString());
    }
    get(k) {
      return this.#p.find((e) => e[0] === k)?.[1] ?? null;
    }
    getAll(k) {
      return this.#p.filter((e) => e[0] === k).map((e) => e[1]);
    }
    has(k) {
      return this.#p.some((e) => e[0] === k);
    }
    set(k, v) {
      const at = this.#p.findIndex((e) => e[0] === k);
      this.#p = this.#p.filter((e, i) => e[0] !== k || i === at);
      if (at < 0) this.#p.push([k, String(v)]);
      else this.#p[this.#p.findIndex((e) => e[0] === k)][1] = String(v);
      this.#changed();
    }
    append(k, v) {
      this.#p.push([k, String(v)]);
      this.#changed();
    }
    delete(k) {
      this.#p = this.#p.filter((e) => e[0] !== k);
      this.#changed();
    }
    forEach(fn) {
      for (const [k, v] of this.#p) fn(v, k, this);
    }
    keys() {
      return this.#p.map((e) => e[0])[Symbol.iterator]();
    }
    values() {
      return this.#p.map((e) => e[1])[Symbol.iterator]();
    }
    entries() {
      return this.#p.map((e) => [...e])[Symbol.iterator]();
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    get size() {
      return this.#p.length;
    }
    sort() {
      this.#p.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      this.#changed();
    }
    toString() {
      const enc = (s) => encodeURIComponent(s).replace(/%20/g, "+");
      return this.#p.map(([k, v]) => `${enc(k)}=${enc(v)}`).join("&");
    }
  }
  g.URLSearchParams = URLSearchParams;

  class URL {
    #parts;
    #params;
    constructor(href, base) {
      const parts = ops.op_rb_url(String(href), base === undefined ? "" : String(base));
      if (!parts) throw new TypeError(`Invalid URL: ${href}`);
      this.#parts = parts;
      this.#params = new URLSearchParams(parts.search);
      this.#params._bind(this);
    }
    static canParse(href, base) {
      return !!ops.op_rb_url(String(href), base === undefined ? "" : String(base));
    }
    static parse(href, base) {
      try {
        return new URL(href, base);
      } catch {
        return null;
      }
    }
    #reparse(href) {
      const parts = ops.op_rb_url(href, "");
      if (parts) this.#parts = parts;
    }
    _setSearch(q) {
      const p = this.#parts;
      this.#reparse(`${p.protocol}//${p.host}${p.pathname}${q ? "?" + q : ""}${p.hash}`.replace(/^([a-z]+:)\/\/(?=\/)/, "$1//"));
    }
    get href() {
      return this.#parts.href;
    }
    set href(v) {
      this.#reparse(String(v));
      this.#params = new URLSearchParams(this.#parts.search);
      this.#params._bind(this);
    }
    get protocol() {
      return this.#parts.protocol;
    }
    get username() {
      return this.#parts.username;
    }
    get password() {
      return this.#parts.password;
    }
    get host() {
      return this.#parts.host;
    }
    get hostname() {
      return this.#parts.hostname;
    }
    get port() {
      return this.#parts.port;
    }
    get pathname() {
      return this.#parts.pathname;
    }
    set pathname(v) {
      const p = this.#parts;
      this.#reparse(new URL(v, p.href).href.split(/[?#]/)[0] + p.search + p.hash);
    }
    get search() {
      return this.#parts.search;
    }
    set search(v) {
      const q = String(v).replace(/^\?/, "");
      this._setSearch(q);
      this.#params = new URLSearchParams(q);
      this.#params._bind(this);
    }
    get searchParams() {
      return this.#params;
    }
    get hash() {
      return this.#parts.hash;
    }
    set hash(v) {
      const h = String(v).replace(/^#/, "");
      this.#reparse(this.#parts.href.split("#")[0] + (h ? "#" + h : ""));
    }
    get origin() {
      return this.#parts.origin;
    }
    toString() {
      return this.href;
    }
    toJSON() {
      return this.href;
    }
    static createObjectURL() {
      return "blob:react-blitz";
    }
    static revokeObjectURL() {}
  }
  g.URL = URL;

  // atob, btoa
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  g.btoa = (s) => {
    let out = "";
    for (let i = 0; i < s.length; i += 3) {
      const [a, b, c] = [s.charCodeAt(i), s.charCodeAt(i + 1), s.charCodeAt(i + 2)];
      const n = (a << 16) | ((b || 0) << 8) | (c || 0);
      out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (isNaN(b) ? "=" : B64[(n >> 6) & 63]) + (isNaN(c) ? "=" : B64[n & 63]);
    }
    return out;
  };
  g.atob = (s) => {
    s = String(s).replace(/[^A-Za-z0-9+/]/g, "");
    let out = "";
    for (let i = 0; i < s.length; i += 4) {
      const n = (B64.indexOf(s[i]) << 18) | (B64.indexOf(s[i + 1]) << 12) | ((B64.indexOf(s[i + 2]) & 63) << 6) | (B64.indexOf(s[i + 3]) & 63);
      out += String.fromCharCode((n >> 16) & 255);
      if (s[i + 2]) out += String.fromCharCode((n >> 8) & 255);
      if (s[i + 3]) out += String.fromCharCode(n & 255);
    }
    return out;
  };

  // crypto
  g.crypto = {
    getRandomValues(arr) {
      ops.op_rb_random(new Uint8Array(arr.buffer, arr.byteOffset, arr.byteLength));
      return arr;
    },
    randomUUID() {
      const b = new Uint8Array(16);
      ops.op_rb_random(b);
      b[6] = (b[6] & 15) | 64;
      b[8] = (b[8] & 63) | 128;
      const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
      return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
    },
  };

  g.self = g;
  g.globalThis = g;
})(globalThis);
