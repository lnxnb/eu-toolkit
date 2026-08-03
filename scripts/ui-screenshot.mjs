// Zero-dependency screenshot of the running EU Toolkit UI-debug instance
// (launched via run-uidebug.bat, which exposes CDP on port 9222).
//
//   node scripts/ui-screenshot.mjs [output.png]
//
// Uses Node 22's built-in fetch + WebSocket to speak the Chrome DevTools
// Protocol directly - no playwright/puppeteer install needed. It holds no
// persistent connection to the webview, so it can never take the app down
// with it (closing WebView2's only page exits the Tauri app).
import { writeFileSync } from "fs";

const out = process.argv[2] ?? "ui-screenshot.png";

let list;
try {
  list = await (await fetch("http://127.0.0.1:9222/json/list")).json();
} catch {
  console.error(
    "CDP endpoint 127.0.0.1:9222 not reachable - is the app running via run-uidebug.bat?",
  );
  process.exit(1);
}
// Prefer the toolkit page (dev server on 1430); fall back to any page target.
const page =
  list.find((t) => t.type === "page" && t.url.includes("localhost:1430")) ??
  list.find((t) => t.type === "page");
if (!page) {
  console.error("No page target found on the CDP endpoint");
  process.exit(1);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});

const pending = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) pending.get(m.id)(m);
};
let nextId = 1;
const call = (method, params = {}) =>
  new Promise((res) => {
    const id = nextId++;
    pending.set(id, res);
    ws.send(JSON.stringify({ id, method, params }));
  });

const shot = await call("Page.captureScreenshot", { format: "png" });
if (!shot.result?.data) {
  console.error("Screenshot failed:", JSON.stringify(shot.error ?? shot));
  process.exit(1);
}
writeFileSync(out, Buffer.from(shot.result.data, "base64"));
console.log(`Saved ${out}  (page: "${page.title}" ${page.url})`);
ws.close();
