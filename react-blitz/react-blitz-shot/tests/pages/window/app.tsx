import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A page drive steers over react-blitz-window's socket: a button whose click shows a badge, a pane
// dragged wider by its handle, and a square a CSS animation slides 10 px a frame.
function App() {
  const [width, setWidth] = useState(100);
  const [clicks, setClicks] = useState(0);
  const drag = useRef<{ x: number; width: number } | null>(null);
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div
      className="stage"
      onPointerMove={(e) => {
        if (drag.current) setWidth(drag.current.width + e.clientX - drag.current.x);
      }}
      onPointerUp={() => {
        drag.current = null;
      }}
    >
      <div className="pane" style={{ width }}>
        <div
          className="handle"
          onPointerDown={(e) => {
            drag.current = { x: e.clientX, width };
          }}
        />
      </div>
      <div className={clicks ? "button on" : "button"} onClick={() => setClicks((n) => n + 1)} />
      <div className="fade">{clicks > 0 && <div className="badge" />}</div>
      <div className="slider" />
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
