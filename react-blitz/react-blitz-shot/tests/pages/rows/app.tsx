import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A scroller of rows 20 px tall with no listener: a wheel changes its offset and nothing else. A row
// under the pointer is wider (`:hover`), and with `?spin` a square beside the scroller spins for good.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div>
      <div className="scroller">
        {Array.from({ length: 40 }, (_, i) => (
          <div key={i} className={`row r${i}`} />
        ))}
      </div>
      {location.search.includes("spin") && <div className="spin" />}
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
