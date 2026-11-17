// react-blitz/client: `createRoot`, as `react-dom/client` gives it.
import type { ReactNode } from "react";
import { flush } from "./bridge";
import { DefaultEventPriority } from "./config";
import type { BlitzNode } from "./node";
import { reconciler } from "./reconciler";

export interface Root {
  render(children: ReactNode): void;
  unmount(): void;
}

export interface RootOptions {
  identifierPrefix?: string;
  onUncaughtError?: (error: unknown, info: unknown) => void;
  onCaughtError?: (error: unknown, info: unknown) => void;
  onRecoverableError?: (error: unknown, info: unknown) => void;
}

const report = (e: unknown) => console.error(e);

export function createRoot(container: BlitzNode, opts: RootOptions = {}): Root {
  const root = reconciler.createContainer(
    container,
    1, // ConcurrentRoot
    null,
    false,
    null,
    opts.identifierPrefix ?? "",
    opts.onUncaughtError ?? report,
    opts.onCaughtError ?? report,
    opts.onRecoverableError ?? report,
    () => {},
    null,
  );
  return {
    render(children) {
      reconciler.updateContainer(children, root, null, null);
    },
    unmount() {
      reconciler.updateContainer(null, root, null, () => flush());
    },
  };
}

export { DefaultEventPriority };
