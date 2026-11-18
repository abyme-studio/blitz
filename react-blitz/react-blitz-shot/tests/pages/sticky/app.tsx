import { useEffect, useRef } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A side card as abyme's chat sets one: a sticky aside of no height at the top of a scroller,
// a card in it, and the scroller scrolled down its rows.
function App() {
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    scroller.current!.scrollTop = 300;
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div ref={scroller} className="scroller">
      <div className="aside">
        <div className="card" />
      </div>
      <div className="rows" />
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
