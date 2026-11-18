import { useEffect } from "react";
import { createRoot } from "react-blitz/client";
import { adopt } from "react-blitz/host";

// A size container and a child sized in its units, as abyme's chat sizes its composer dock
// (`height: 100cqh`); and a rule on the container's width; and a container named by the
// `container` shorthand, as abyme's `Page` is (`container: page / inline-size`).
function App() {
  useEffect(() => {
    document.documentElement.setAttribute("data-preview-ready", "");
  }, []);
  return (
    <div data-preview-frame="desktop">
      <div className="box">
        <div className="fill" />
        <div className="ruled" />
      </div>
      <div className="page">
        <div className="named" />
      </div>
    </div>
  );
}

createRoot(adopt("body") as any).render(<App />);
