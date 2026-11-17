// React props to ops: attributes as SET_ATTR, `style` as one SET_STYLE per property, handlers as
// LISTEN. React DOM's names and value rules, so the same JSX gives the same attributes.
import { encoder, written } from "./bridge";
import { BlitzElement, cssName, eventOfProp } from "./node";

const SKIP = new Set(["children", "key", "ref", "dangerouslySetInnerHTML", "suppressContentEditableWarning", "suppressHydrationWarning", "defaultValue", "defaultChecked", "innerHTML"]);

const HTML_NAMES: Record<string, string> = {
  className: "class",
  htmlFor: "for",
  httpEquiv: "http-equiv",
  acceptCharset: "accept-charset",
};

const BOOLEAN = new Set([
  "allowfullscreen", "async", "autofocus", "autoplay", "checked", "controls", "default", "defer", "disabled",
  "disablepictureinpicture", "disableremoteplayback", "formnovalidate", "hidden", "inert", "loop", "multiple",
  "muted", "nomodule", "novalidate", "open", "playsinline", "readonly", "required", "reversed", "scoped",
  "seamless", "selected", "itemscope",
]);

// SVG attributes whose names keep their capitals.
const SVG_CAMEL = new Set([
  "viewBox", "preserveAspectRatio", "gradientUnits", "gradientTransform", "patternUnits", "patternContentUnits",
  "patternTransform", "clipPathUnits", "maskUnits", "maskContentUnits", "markerUnits", "markerWidth",
  "markerHeight", "refX", "refY", "spreadMethod", "stdDeviation", "textLength", "lengthAdjust", "startOffset",
  "baseFrequency", "numOctaves", "pathLength", "filterUnits", "primitiveUnits", "tableValues", "kernelMatrix",
  "diffuseConstant", "specularConstant", "specularExponent", "surfaceScale", "edgeMode", "xChannelSelector",
  "yChannelSelector", "attributeName", "repeatCount", "keyTimes", "keySplines", "calcMode",
]);

/** The attribute a React prop writes, or null when it is no attribute. */
export function attrName(prop: string, svg: boolean): string | null {
  if (SKIP.has(prop) || eventOfProp(prop)) return null;
  if (prop === "style") return null;
  const named = HTML_NAMES[prop];
  if (named) return named;
  if (prop.startsWith("data-") || prop.startsWith("aria-")) return prop;
  if (svg) {
    if (SVG_CAMEL.has(prop)) return prop;
    if (prop === "xlinkHref") return "href";
    if (prop.startsWith("xml") || prop.startsWith("xlink")) return prop.replace(/^(xml|xlink)([A-Z])/, (_m, a, c) => `${a}:${c.toLowerCase()}`);
    return prop.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
  }
  return prop.toLowerCase();
}

/** The attribute's value, or null when React DOM would leave it out. */
export function attrValue(name: string, value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "function" || typeof value === "symbol") return null;
  if (typeof value === "boolean") {
    if (name.startsWith("data-") || name.startsWith("aria-")) return String(value);
    if (BOOLEAN.has(name)) return value ? "" : null;
    if (["draggable", "spellcheck", "contenteditable"].includes(name)) return String(value);
    return null;
  }
  return String(value);
}

type Style = Record<string, unknown> | null | undefined;

function diffStyle(el: BlitzElement, prev: Style, next: Style): void {
  for (const k in prev ?? {}) if (!next || !(k in next)) el.style.removeProperty(cssName(k));
  for (const k in next ?? {}) {
    const v = next![k];
    if (prev && prev[k] === v) continue;
    if (v === null || v === undefined || v === "" || typeof v === "boolean") el.style.removeProperty(cssName(k));
    else el.style.setProperty(cssName(k), v);
  }
}

function events(props: Record<string, any>): Set<string> {
  const out = new Set<string>();
  for (const k in props) {
    if (typeof props[k] !== "function") continue;
    const e = eventOfProp(k);
    if (e) out.add(e.type);
  }
  return out;
}

/** Write what changed from `prev` to `next`. */
export function applyProps(el: BlitzElement, prev: Record<string, any>, next: Record<string, any>): void {
  const svg = el.namespaceURI === "http://www.w3.org/2000/svg";
  for (const k in prev) {
    if (k in next) continue;
    const name = attrName(k, svg);
    if (name) el.removeAttribute(name);
  }
  for (const k in next) {
    const v = next[k];
    if (prev[k] === v && k !== "value" && k !== "checked") continue;
    if (k === "style") {
      diffStyle(el, prev.style, v);
      continue;
    }
    if (k === "value" && v !== undefined && v !== null) {
      el.value = String(v);
      continue;
    }
    if (k === "checked") {
      el.checked = !!v;
    }
    if (k === "disabled") el.disabled = !!v;
    const name = attrName(k, svg);
    if (!name) continue;
    const value = attrValue(name, v);
    if (value === null) el.removeAttribute(name);
    else el.setAttribute(name, value);
  }

  const before = events(prev);
  const after = events(next);
  el.props = next;
  for (const t of after) {
    if (before.has(t) || el.listeners.has(t)) continue;
    encoder.listen(el.rbId, t, true);
    written();
  }
  for (const t of before) {
    if (after.has(t) || el.listeners.has(t)) continue;
    encoder.listen(el.rbId, t, false);
    written();
  }
}

/** The props an element mounts with: `defaultValue` and `defaultChecked` count once. */
export function initialProps(el: BlitzElement, props: Record<string, any>): void {
  applyProps(el, {}, props);
  if (props.value === undefined && props.defaultValue !== undefined) el.value = String(props.defaultValue);
  if (props.checked === undefined && props.defaultChecked !== undefined) el.checked = !!props.defaultChecked;
}
