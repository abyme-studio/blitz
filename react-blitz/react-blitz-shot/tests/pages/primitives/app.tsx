import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// One primitive of the quads back end per page, chosen by the query's `p`, for a pixel test
// against Vello: each at 40 px from the top left, on the page's background.
const image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAYAAADED76LAAAAJElEQVR4nGOQz3/9Hxn/v2yLghnooEB3id9/ZHxeSQ8F00EBAGKznCF6AIojAAAAAElFTkSuQmCC";

const kinds: Record<string, () => unknown> = {
  rect: () => <div className="rect" />,
  rounded: () => <div className="rounded" />,
  border: () => <div className="border" />,
  shadow: () => <div className="shadow" />,
  inset: () => <div className="inset" />,
  clip: () => <div className="clip"><div /></div>,
  "rounded-clip": () => <div className="rounded-clip"><div /></div>,
  image: () => <img className="image" src={image} />,
  opacity: () => <div className="opacity"><div /></div>,
  text: () => <span className="text">A run of text, drawn twice</span>,
  emoji: () => <span className="emoji">😀🎉</span>,
  path: () => (
    <svg className="path" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={3} strokeLinecap="round">
      <path d="M4 18 C8 4, 16 4, 20 18" />
    </svg>
  ),
  gradient: () => <div className="gradient" />,
  filter: () => <div className="filter" />,
};

function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  const p = /[?&]p=([\w-]+)/.exec(location.search)?.[1] ?? "rect";
  return <div className="frame">{kinds[p]()}</div>;
}

createRoot(adopt("body") as any).render(<App />);
