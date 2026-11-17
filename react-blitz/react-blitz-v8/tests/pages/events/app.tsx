import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

function App() {
  const [clicks, setClicks] = useState(0);
  const [width, setWidth] = useState(100);
  const [seen, setSeen] = useState(0);
  const [read, setRead] = useState(0);
  const watched = useRef<HTMLParagraphElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const measured = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const o = new ResizeObserver((entries) => setSeen(entries[0]!.contentRect.width));
    o.observe(box.current!);
    return () => o.disconnect();
  }, []);

  useEffect(() => {
    // Says what it last heard the watched text become; written only when that changed, so its own
    // write does not wake it again and again.
    const o = new MutationObserver(() => {
      const heard = document.getElementById("heard")!;
      const now = `heard ${watched.current!.textContent}`;
      if (heard.textContent !== now) heard.textContent = now;
    });
    o.observe(watched.current!, { subtree: true, childList: true, characterData: true });
    return () => o.disconnect();
  }, []);

  return (
    <div>
      <button id="count" onClick={() => setClicks((c) => c + 1)}>
        count
      </button>
      <p id="clicks" ref={watched}>{`clicks ${clicks}`}</p>
      <p id="heard">heard 0</p>
      <button id="trap" onKeyDown={(e) => e.key === "Tab" && e.preventDefault()}>
        trap
      </button>
      <button id="free">free</button>
      <button id="widen" onClick={() => setWidth(300)}>
        widen
      </button>
      <div id="box" ref={box} style={{ width }} />
      <p id="seen">{`seen ${seen}`}</p>
      <button
        id="write-read"
        onClick={() => {
          measured.current!.style.width = "250px";
          setRead(measured.current!.getBoundingClientRect().width);
        }}
      >
        write then read
      </button>
      <div id="measured" ref={measured} />
      <p id="read">{`read ${read}`}</p>
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
