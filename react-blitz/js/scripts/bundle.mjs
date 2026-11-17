// Bundle a test page for react-blitz's engine tests: `node scripts/bundle.mjs <page dir> <out dir>`.
// The page dir holds index.html and app.tsx; the out dir gets app.js and the page's other files, React and
// react-blitz bundled in, react-blitz/install first.
import { build } from "esbuild";
import { copyFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const [page, out] = process.argv.slice(2);
if (!page || !out) throw new Error("usage: node scripts/bundle.mjs <page dir> <out dir>");
const src = join(dirname(fileURLToPath(import.meta.url)), "../src");
mkdirSync(out, { recursive: true });
await build({
  stdin: {
    contents: `import "react-blitz/install"; import "./app.tsx";`,
    resolveDir: resolve(page),
    loader: "ts",
  },
  bundle: true,
  format: "esm",
  outfile: join(out, "app.js"),
  jsx: "automatic",
  platform: "neutral",
  mainFields: ["module", "main"],
  define: { "process.env.NODE_ENV": '"production"' },
  nodePaths: [join(src, "../node_modules")],
  alias: {
    "react-blitz/install": join(src, "install.ts"),
    "react-blitz/client": join(src, "client.ts"),
    "react-blitz/dom": join(src, "dom.ts"),
    "react-blitz/host": join(src, "host.ts"),
    "react-blitz": join(src, "index.ts"),
  },
  logLevel: "warning",
});
// index.html and whatever else the page holds (stylesheets, fonts), beside the bundle.
for (const file of readdirSync(page)) {
  if (!/\.tsx?$/.test(file)) copyFileSync(join(page, file), join(out, file));
}
