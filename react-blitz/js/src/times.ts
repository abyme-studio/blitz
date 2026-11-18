// Where a frame's JS goes, by listener: on when the page's query holds `rb-times`. Each listener,
// observer callback, frame callback and timer is timed, keyed by what it is and the start of its
// source, and every two seconds one `rb-times` line says each key's runs and ms since the last.
// Off, `timed` is one check and the call.
let on: boolean | null = null;
const sums = new Map<string, { n: number; ms: number }>();
let frames = 0;
let last = 0;

function isOn(): boolean {
  if (on === null) {
    const search = (globalThis as any).location?.search;
    if (typeof search !== "string") return false;
    on = /[?&]rb-times\b/.test(search);
    if (on) wrapTimers();
  }
  return on;
}

const snippet = (fn: unknown): string => (typeof fn === "function" ? `${fn.name || "anonymous"} ${String(fn).replace(/\s+/g, " ").slice(0, 90)}` : "object");

/** Run `fn`, adding its time under `kind` and the listener's source when times are on. */
export function timed<T>(kind: string, listener: unknown, fn: () => T): T {
  if (!isOn()) return fn();
  const start = performance.now();
  try {
    return fn();
  } finally {
    const key = listener === null ? kind : `${kind} | ${snippet(listener)}`;
    const s = sums.get(key) ?? { n: 0, ms: 0 };
    s.n++;
    s.ms += performance.now() - start;
    sums.set(key, s);
  }
}

/** A frame went by: every two seconds, say the sums and start again. */
export function frameTimed(): void {
  if (!isOn()) return;
  frames++;
  const now = performance.now();
  if (now - last < 2000) return;
  const rows = [...sums].map(([key, s]) => ({ key, n: s.n, ms: Math.round(s.ms * 100) / 100 })).sort((a, b) => b.ms - a.ms);
  if (rows.length) console.log(`rb-times ${JSON.stringify({ frames, rows })}`);
  sums.clear();
  frames = 0;
  last = now;
}

function wrapTimers(): void {
  const g = globalThis as any;
  for (const name of ["setTimeout", "setInterval"]) {
    const real = g[name];
    if (typeof real !== "function") continue;
    g[name] = (fn: unknown, ...rest: unknown[]) => real(typeof fn === "function" ? (...a: unknown[]) => timed(name, fn, () => (fn as any)(...a)) : fn, ...rest);
  }
}
