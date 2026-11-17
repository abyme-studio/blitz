// react-blitz/dom: what `react-dom` gives that abyme's code and Radix import.
import type { ReactNode, ReactPortal } from "react";
import { flush } from "./bridge";
import type { BlitzNode } from "./node";
import { reconciler } from "./reconciler";

const REACT_PORTAL_TYPE = Symbol.for("react.portal");

export function createPortal(children: ReactNode, container: BlitzNode, key?: string | null): ReactPortal {
  return {
    $$typeof: REACT_PORTAL_TYPE,
    key: key == null ? null : String(key),
    children,
    containerInfo: container,
    implementation: null,
  } as unknown as ReactPortal;
}

export function flushSync<R>(fn?: () => R): R {
  const r = reconciler.flushSyncFromReconciler ? reconciler.flushSyncFromReconciler(fn) : reconciler.flushSync(fn);
  reconciler.flushSyncWork?.();
  flush();
  return r as R;
}

export const unstable_batchedUpdates = <A, R>(fn: (a: A) => R, a: A): R => reconciler.batchedUpdates(fn, a);
export const version = "19.3.0";
export function preload(): void {}
export function preinit(): void {}
export function preconnect(): void {}
export function prefetchDNS(): void {}
export function preloadModule(): void {}
export function preinitModule(): void {}
export function requestFormReset(): void {}
export function useFormStatus() {
  return { pending: false, data: null, method: null, action: null };
}
export function useFormState<S>(_action: unknown, initial: S): [S, () => void, boolean] {
  return [initial, () => {}, false];
}

export default { createPortal, flushSync, unstable_batchedUpdates, version, preload, preinit, preconnect, prefetchDNS, preloadModule, preinitModule, requestFormReset, useFormStatus, useFormState };
