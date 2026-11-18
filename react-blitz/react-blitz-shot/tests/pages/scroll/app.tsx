import { useEffect, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A scroller whose scroll listener says how often it ran (`.count`, 10 px a run) and the offset it
// read on its last run (`.top`, 1 px a pixel).
function App() {
  const [count, setCount] = useState(0);
  const [top, setTop] = useState(0);
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div>
      <div
        className="scroller"
        onScroll={(e) => {
          const at = e.currentTarget.scrollTop;
          setCount((n) => n + 1);
          setTop(at);
        }}
      >
        <div className="long" />
      </div>
      <div className="count" style={{ width: count * 10 }} />
      <div className="top" style={{ width: top }} />
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
