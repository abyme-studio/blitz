import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

function App() {
  // Ready only once React has committed and its effects ran, as abyme's preview page does.
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div className="card" id="card">
      <span>A still page, shot twice</span>
      <div className="row">
        <div className="chip" />
        <div className="chip wide" />
      </div>
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
