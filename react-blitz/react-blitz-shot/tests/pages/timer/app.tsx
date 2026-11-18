import { useEffect, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

/** A page with no timer of its own. `arm(ms)` sets one, and the page then says how late it fired. */
function App() {
  const [late, setLate] = useState("none");
  useEffect(() => {
    (globalThis as any).arm = (ms: number) => {
      const set = performance.now();
      setTimeout(() => setLate(String(performance.now() - set - ms)), ms);
    };
  }, []);
  return <div id="late">{late}</div>;
}

createRoot(adopt("body") as any).render(<App />);
