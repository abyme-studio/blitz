// ANSWERS: the DOM members react-blitz's nodes, `document` and `window` answer, by the names
// abyme's docs/cells/host.md uses in its behaviour library's table. abyme's rules test fails a
// call that table names and this list lacks.
export const ANSWERS: readonly string[] = [
  // document
  "document", "document.activeElement", "document.body", "document.head", "document.createElement",
  "documentElement", "createTreeWalker", "createElement", "appendChild",
  // listeners and events
  "addEventListener", "removeEventListener", "dispatchEvent", "focusin", "focusout", "pointerdown",
  "pointerleave", "click", "keydown", "blur", "scroll", "wheel", "resize", "touchstart", "touchend",
  "animationstart", "animationend", "animationcancel", "load", "error", "reset",
  // observers and frames
  "MutationObserver", "ResizeObserver", "IntersectionObserver", "requestAnimationFrame",
  // style and layout
  "getComputedStyle", "getBoundingClientRect", "getClientRects", "offsetWidth", "offsetHeight",
  "offsetParent", "innerWidth", "innerHeight", "clientHeight", "clientWidth", "scrollHeight",
  "scrollWidth", "scrollTop", "scrollLeft", "scrollTo", "scrollIntoView", "visualViewport",
  // focus, pointer, attributes
  ".focus()", "focus", "setPointerCapture", "releasePointerCapture", "hasPointerCapture", "inert",
  "aria-hidden",
];
