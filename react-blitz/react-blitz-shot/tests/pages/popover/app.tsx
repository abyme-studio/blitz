import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A button opens an auto popover by showPopover; a click on the open popover marks it `hit`.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <>
      <div className="clip">
        <button id="open" onClick={() => (document.getElementById("pop") as any).showPopover()}>
          Open
        </button>
        <div id="pop" popover="auto" onClick={(e) => (e.currentTarget as any).classList.add("hit")} />
      </div>
      <div className="cover" />
    </>
  );
}

createRoot(adopt("body") as any).render(<App />);
