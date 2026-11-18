import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// Four scrollers 200px wide, each with one row: `scrollbar-gutter: stable` keeps the scrollbar's
// width free at the right, as abyme's transcript does (`[scrollbar-gutter:stable]`), by the
// scrollbar's width; a scroller without it keeps none.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div data-preview-frame="desktop">
      {["thin", "wide", "none", "plain"].map((name) => (
        <div key={name} className={`scroller ${name}`}>
          <div className="row" />
        </div>
      ))}
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
