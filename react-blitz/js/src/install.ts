// react-blitz/install: the page's `document`, `window` and DOM classes, and the handlers the host
// calls. A native build imports this before anything else, so libraries that test for a DOM at
// load find one.
import { BlitzDocument, NodeFilter, TreeWalker, makeDocument } from "./document";
import { dispatchFromHost } from "./events";
import * as host from "./host";
import { BlitzElement, BlitzFragment, BlitzNode, BlitzRect, BlitzText } from "./node";
import { installWindow } from "./window";

function eventClasses(g: any): void {
  const Event = g.Event;
  const withFields = (defaults: Record<string, unknown>) =>
    class extends Event {
      constructor(type: string, init: Record<string, unknown> = {}) {
        super(type, init);
        for (const k in defaults) (this as any)[k] = k in init ? init[k] : defaults[k];
      }
    };
  const mods = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };
  const mouse = { ...mods, clientX: 0, clientY: 0, pageX: 0, pageY: 0, screenX: 0, screenY: 0, offsetX: 0, offsetY: 0, movementX: 0, movementY: 0, button: 0, buttons: 0, relatedTarget: null, detail: 1, view: null };
  g.UIEvent = withFields({ detail: 0, view: null });
  g.MouseEvent = withFields(mouse);
  g.PointerEvent = withFields({ ...mouse, pointerId: 1, pointerType: "mouse", isPrimary: true, pressure: 0, width: 1, height: 1, tiltX: 0, tiltY: 0 });
  g.WheelEvent = withFields({ ...mouse, deltaX: 0, deltaY: 0, deltaZ: 0, deltaMode: 0 });
  g.KeyboardEvent = withFields({ ...mods, key: "", code: "", repeat: false, isComposing: false, location: 0, charCode: 0, keyCode: 0, which: 0 });
  g.FocusEvent = withFields({ relatedTarget: null });
  g.InputEvent = withFields({ data: null, inputType: "", isComposing: false });
  g.TouchEvent = withFields({ ...mods, touches: [], targetTouches: [], changedTouches: [] });
  g.AnimationEvent = withFields({ animationName: "", elapsedTime: 0, pseudoElement: "" });
  g.TransitionEvent = withFields({ propertyName: "", elapsedTime: 0, pseudoElement: "" });
  g.ToggleEvent = withFields({ oldState: "", newState: "", source: null });
  g.KeyboardEvent.DOM_KEY_LOCATION_STANDARD = 0;
  g.Event.prototype.getModifierState = function (this: any, k: string) {
    return { Shift: this.shiftKey, Control: this.ctrlKey, Alt: this.altKey, Meta: this.metaKey }[k] ?? false;
  };
}

function domClasses(g: any): void {
  g.Node = BlitzNode;
  g.Element = BlitzElement;
  g.HTMLElement = BlitzElement;
  g.SVGElement = BlitzElement;
  g.MathMLElement = BlitzElement;
  for (const name of ["Div", "Span", "Input", "TextArea", "Select", "Button", "Anchor", "Image", "Canvas", "Form", "Label", "Iframe", "Template", "Style", "Script", "Body", "Head", "Html", "Video", "Audio", "Media", "Option", "Paragraph", "Pre", "Table", "LI", "UList", "OList", "Heading", "Details", "Dialog"]) {
    g[`HTML${name}Element`] = BlitzElement;
  }
  g.Text = BlitzText;
  g.CharacterData = BlitzText;
  g.Comment = BlitzText;
  g.Document = BlitzDocument;
  g.HTMLDocument = BlitzDocument;
  g.DocumentFragment = BlitzFragment;
  g.ShadowRoot = class ShadowRoot {};
  g.DOMRect = BlitzRect;
  g.DOMRectReadOnly = BlitzRect;
  g.NodeFilter = NodeFilter;
  g.TreeWalker = TreeWalker;
  g.Window = Object;
}

/** Install the page's DOM on the global object. Safe to call twice. */
export function install(): void {
  const g = globalThis as any;
  if (g.__reactBlitzInstalled) return;
  g.__reactBlitzInstalled = true;
  eventClasses(g);
  domClasses(g);
  g.document = makeDocument();
  installWindow();
  g.__reactBlitzHandlers = {
    dispatch: dispatchFromHost,
    frame: host.frame,
  };
}

install();
