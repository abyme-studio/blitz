import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// An icon as lucide-react draws one, its colour mixed in oklab (index.html): stroked paths at 24 units in currentColor, sized by a class.
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <svg className="icon" xmlns="http://www.w3.org/2000/svg" width={24} height={24} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={4} strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </svg>
  );
}

createRoot(adopt("body") as any).render(<App />);
