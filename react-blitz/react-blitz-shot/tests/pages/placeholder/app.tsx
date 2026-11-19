import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// An empty textarea with a placeholder, and an input whose value is set: only the first shows its
// placeholder (red) and matches :placeholder-shown (a blue border; green otherwise).
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <>
      <textarea id="empty" placeholder="WWWW" />
      <input id="filled" placeholder="WWWW" defaultValue="ok" />
    </>
  );
}

createRoot(adopt("body") as any).render(<App />);
