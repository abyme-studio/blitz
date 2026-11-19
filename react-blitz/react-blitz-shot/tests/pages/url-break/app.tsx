import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A URL too long for its box: Chromium breaks it after the "?", never after a "/" before a letter.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return <p>abyme:file?path=/abyme/fixtures/README.md</p>;
}

createRoot(adopt("body") as any).render(<App />);
