// The wire format's JS half: ops as u32 words, strings interned per commit. `blitz-commit` owns
// the format (react-blitz/blitz-commit/src/ops.rs); the golden files hold the two to it.

export const OP = {
  CREATE_ELEMENT: 1,
  CREATE_TEXT: 2,
  SET_TEXT: 3,
  SET_ATTR: 4,
  REMOVE_ATTR: 5,
  SET_STYLE: 6,
  REMOVE_STYLE: 7,
  APPEND: 8,
  INSERT_BEFORE: 9,
  REMOVE: 10,
  DROP: 11,
  LISTEN: 12,
} as const;

export const NS = { html: 0, svg: 1, mathml: 2 } as const;

const utf8 = new TextEncoder();

export interface Commit {
  ops: Uint32Array;
  strings: Uint8Array;
}

export class Encoder {
  private words: number[] = [];
  private table: string[] = [];
  private index = new Map<string, number>();

  get empty(): boolean {
    return this.words.length === 0;
  }

  str(s: string): number {
    let i = this.index.get(s);
    if (i === undefined) {
      i = this.table.length;
      this.table.push(s);
      this.index.set(s, i);
    }
    return i;
  }

  createElement(id: number, tag: string, ns: number): void {
    this.words.push(OP.CREATE_ELEMENT, id, this.str(tag), ns);
  }
  createText(id: number, text: string): void {
    this.words.push(OP.CREATE_TEXT, id, this.str(text));
  }
  setText(id: number, text: string): void {
    this.words.push(OP.SET_TEXT, id, this.str(text));
  }
  setAttr(id: number, name: string, value: string): void {
    this.words.push(OP.SET_ATTR, id, this.str(name), this.str(value));
  }
  removeAttr(id: number, name: string): void {
    this.words.push(OP.REMOVE_ATTR, id, this.str(name));
  }
  setStyle(id: number, property: string, value: string): void {
    this.words.push(OP.SET_STYLE, id, this.str(property), this.str(value));
  }
  removeStyle(id: number, property: string): void {
    this.words.push(OP.REMOVE_STYLE, id, this.str(property));
  }
  append(parent: number, child: number): void {
    this.words.push(OP.APPEND, parent, child);
  }
  insertBefore(parent: number, child: number, before: number): void {
    this.words.push(OP.INSERT_BEFORE, parent, child, before);
  }
  remove(parent: number, child: number): void {
    this.words.push(OP.REMOVE, parent, child);
  }
  drop(id: number): void {
    this.words.push(OP.DROP, id);
  }
  listen(id: number, event: string, on: boolean): void {
    this.words.push(OP.LISTEN, id, this.str(event), on ? 1 : 0);
  }

  /** The commit so far, and a fresh table for the next. */
  take(): Commit {
    const ops = Uint32Array.from(this.words);
    let size = 0;
    const bytes = this.table.map((s) => {
      const b = utf8.encode(s);
      size += 4 + b.length;
      return b;
    });
    const strings = new Uint8Array(size);
    const view = new DataView(strings.buffer);
    let at = 0;
    for (const b of bytes) {
      view.setUint32(at, b.length, true);
      strings.set(b, at + 4);
      at += 4 + b.length;
    }
    this.words = [];
    this.table = [];
    this.index.clear();
    return { ops, strings };
  }
}
