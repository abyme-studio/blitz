import { useEffect, useState } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

/** A timer always pending, as a real page has, and one that falls due soon. */
function Turns() {
  const [fired, setFired] = useState(false);
  useEffect(() => {
    const every = setInterval(() => {}, 1000);
    const soon = setTimeout(() => setFired(true), 30);
    return () => (clearInterval(every), clearTimeout(soon));
  }, []);
  return <div id="timer">{fired ? "fired" : "waiting"}</div>;
}

createRoot(adopt("body") as any).render(<Turns />);
