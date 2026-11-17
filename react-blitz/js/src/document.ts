// `document`: the page's own document node, with the members abyme's libraries call.
import { read } from "./bridge";
import { NS } from "./encoder";
import * as host from "./host";
import { BlitzElement, BlitzFragment, BlitzNode, BlitzText, newId, nodes } from "./node";

const NS_OF: Record<string, number> = {
  "http://www.w3.org/1999/xhtml": NS.html,
  "http://www.w3.org/2000/svg": NS.svg,
  "http://www.w3.org/1998/Math/MathML": NS.mathml,
};

/** NodeFilter's answers and masks. */
export const NodeFilter = {
  FILTER_ACCEPT: 1,
  FILTER_REJECT: 2,
  FILTER_SKIP: 3,
  SHOW_ALL: 0xffffffff,
  SHOW_ELEMENT: 0x1,
  SHOW_TEXT: 0x4,
};

type Filter = ((n: BlitzNode) => number) | { acceptNode(n: BlitzNode): number } | null;

/** A TreeWalker over JS's tree, as Radix's focus scope walks it. */
export class TreeWalker {
  currentNode: BlitzNode;
  constructor(readonly root: BlitzNode, readonly whatToShow: number, readonly filter: Filter) {
    this.currentNode = root;
  }
  private accept(n: BlitzNode): number {
    const bit = n.nodeType === 1 ? 0x1 : n.nodeType === 3 ? 0x4 : 0;
    if (!(this.whatToShow & bit)) return NodeFilter.FILTER_SKIP;
    if (!this.filter) return NodeFilter.FILTER_ACCEPT;
    return typeof this.filter === "function" ? this.filter(n) : this.filter.acceptNode(n);
  }
  private kids(n: BlitzNode): BlitzNode[] {
    return n instanceof BlitzElement && n.adopted && n.childNodes.length === 0 ? host.children(n) : n.childNodes;
  }
  nextNode(): BlitzNode | null {
    let n: BlitzNode | null = this.currentNode;
    let skipChildren = false;
    while (n) {
      const kids: BlitzNode[] = skipChildren ? [] : this.kids(n);
      if (kids.length) n = kids[0]!;
      else {
        while (n && n !== this.root && !n.nextSibling) n = n.parentNode;
        if (!n || n === this.root) return null;
        n = n.nextSibling!;
      }
      const r: number = this.accept(n!);
      skipChildren = r === NodeFilter.FILTER_REJECT;
      if (r === NodeFilter.FILTER_ACCEPT) {
        this.currentNode = n!;
        return n;
      }
    }
    return null;
  }
  parentNode(): BlitzNode | null {
    for (let n = this.currentNode.parentNode; n && n !== this.root.parentNode; n = n.parentNode) {
      if (this.accept(n) === NodeFilter.FILTER_ACCEPT) return (this.currentNode = n);
    }
    return null;
  }
  firstChild(): BlitzNode | null {
    for (const c of this.kids(this.currentNode)) if (this.accept(c) === NodeFilter.FILTER_ACCEPT) return (this.currentNode = c);
    return null;
  }
}

export class BlitzDocument extends BlitzNode {
  readonly nodeType = 9;
  readonly nodeName = "#document";
  readyState = "complete";
  visibilityState = "visible";
  hidden = false;
  cookie = "";
  compatMode = "CSS1Compat";
  characterSet = "UTF-8";
  readonly fonts = {
    ready: Promise.resolve(),
    status: "loaded",
    check: () => true,
    load: () => Promise.resolve([]),
    addEventListener() {},
    removeEventListener() {},
    forEach() {},
    [Symbol.iterator]: function* () {},
  };

  constructor(id: number) {
    super(id);
  }

  override get ownerDocument(): null {
    return null;
  }
  override get isConnected(): boolean {
    return true;
  }
  get defaultView(): typeof globalThis {
    return globalThis;
  }
  get documentElement(): BlitzElement {
    return host.adopt("html") as BlitzElement;
  }
  get head(): BlitzElement {
    return host.adopt("head") as BlitzElement;
  }
  get body(): BlitzElement {
    return host.adopt("body") as BlitzElement;
  }
  get scrollingElement(): BlitzElement {
    return this.documentElement;
  }
  get activeElement(): BlitzElement | null {
    return host.focused() ?? this.body;
  }
  get title(): string {
    return host.find(this, "title")?.textContent ?? "";
  }
  set title(_t: string) {}
  get location(): any {
    return (globalThis as any).location;
  }
  get styleSheets(): unknown[] {
    return [];
  }
  hasFocus(): boolean {
    return host.hasFocus();
  }
  createElement(tag: string): BlitzElement {
    return new BlitzElement(newId(), String(tag).toLowerCase(), NS.html);
  }
  createElementNS(ns: string | null, tag: string): BlitzElement {
    return new BlitzElement(newId(), tag, NS_OF[ns ?? ""] ?? NS.html);
  }
  createTextNode(text: string): BlitzText {
    return new BlitzText(newId(), String(text));
  }
  createComment(_text: string): BlitzText {
    return new BlitzText(newId(), "");
  }
  createDocumentFragment(): BlitzFragment {
    return new BlitzFragment();
  }
  createTreeWalker(root: BlitzNode, whatToShow = NodeFilter.SHOW_ALL, filter: Filter = null): TreeWalker {
    return new TreeWalker(root, whatToShow, filter);
  }
  createEvent(_kind: string): any {
    const e = new (globalThis as any).Event("");
    e.initEvent = (type: string, bubbles: boolean, cancelable: boolean) => Object.assign(e, { type, bubbles, cancelable });
    return e;
  }
  createRange(): any {
    return {
      setStart() {},
      setEnd() {},
      selectNode() {},
      selectNodeContents() {},
      collapse() {},
      getBoundingClientRect: () => new (globalThis as any).DOMRect(),
      getClientRects: () => [],
      commonAncestorContainer: this.body,
    };
  }
  getElementById(id: string): BlitzElement | null {
    return host.find(this, `#${cssEscape(id)}`);
  }
  getElementsByTagName(tag: string): BlitzElement[] {
    return host.findAll(this, tag);
  }
  getElementsByClassName(names: string): BlitzElement[] {
    return host.findAll(this, names.trim().split(/\s+/).map((n) => `.${cssEscape(n)}`).join(""));
  }
  querySelector(selector: string): BlitzElement | null {
    return host.find(this, selector);
  }
  querySelectorAll(selector: string): BlitzElement[] {
    return host.findAll(this, selector);
  }
  elementFromPoint(x: number, y: number): BlitzElement | null {
    return host.at(x, y);
  }
  elementsFromPoint(x: number, y: number): BlitzElement[] {
    const e = host.at(x, y);
    const out: BlitzElement[] = [];
    for (let n: BlitzNode | null = e; n; n = n.parentNode) if (n instanceof BlitzElement) out.push(n);
    return out;
  }
  getSelection(): any {
    return (globalThis as any).getSelection();
  }
  execCommand(): boolean {
    return false;
  }
  protected shallowCopy(): BlitzNode {
    return this;
  }
}

export function cssEscape(s: string): string {
  return String(s).replace(/([^\w-])/g, "\\$1").replace(/^(\d)/, "\\3$1 ");
}

export function makeDocument(): BlitzDocument {
  const id = read().adopt("document");
  nodes.delete(id);
  return new BlitzDocument(id);
}
