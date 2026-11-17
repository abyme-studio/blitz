import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

createRoot(adopt("body") as any).render(
  <div className="outer">
    <div className="inner a" />
    <div className="inner wide b">
      <span>text</span>
    </div>
  </div>,
);
