import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A preview frame and, after it in the body, a portal: as abyme's preview page holds a story and its popups.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <>
      <div data-preview-frame="desktop">
        <div className="a" />
        <div className="row">
          <div />
          <section />
        </div>
      </div>
      <div className="pop" />
    </>
  );
}

createRoot(adopt("body") as any).render(<App />);
