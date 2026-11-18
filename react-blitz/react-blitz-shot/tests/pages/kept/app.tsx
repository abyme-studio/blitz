import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// Two scrollers side by side, each of 40 rows of text 20 px tall, with no listener: a wheel
// changes one's offset and nothing else, and a row of the second under the pointer changes
// colour (`:hover`).
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <>
      {["a", "b"].map((name) => (
        <div key={name} className="scroller" id={name}>
          {Array.from({ length: 40 }, (_, i) => (
            <div key={i} className="row">
              {name} row {i}
            </div>
          ))}
        </div>
      ))}
    </>
  );
}

createRoot(adopt("body") as any).render(<App />);
