import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// One scroller of 60 rows that all say the same words: a scroll brings rows into view, and no
// glyph the first frame did not draw.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div className="scroller">
      {Array.from({ length: 60 }, (_, i) => (
        <div key={i} className="row">
          The same words in every row
        </div>
      ))}
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
