import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// What the quads back end draws on the CPU: a gradient, which a scroll of 40 px keeps whole in
// view; and a backdrop filter, which it draws without and logs.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div className="scroller">
      <div className="gradient" />
      <div className="frosted" />
      <div className="tall" />
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
