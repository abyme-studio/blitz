import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A scroller whose page code sets `scrollTop`: its `scroll` listener hears it, as a browser's does,
// and the mark grows to the offset it read. abyme's transcript (virtua) draws its rows from that event.
function App() {
  const scroller = useRef<HTMLDivElement>(null);
  const [heard, setHeard] = useState(0);
  useEffect(() => {
    const el = scroller.current!;
    el.addEventListener("scroll", () => setHeard(el.scrollTop));
    el.scrollTop = 60;
  }, []);
  useEffect(() => {
    if (heard) document.documentElement.setAttribute("data-preview-ready", "");
  }, [heard]);
  return (
    <div data-preview-frame="desktop">
      <div ref={scroller} className="scroller">
        <div className="tall" />
      </div>
      <div className="mark" style={{ width: heard }} />
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
