// Tiny CDP client for the SavePrism WebView2 (debug port 9222).
// node cdp.mjs eval "<js expression, may use await>"
// node cdp.mjs shot <out.png>
import { writeFileSync } from "node:fs";

const [cmd, arg] = process.argv.slice(2);
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const n = ++id;
    pending.set(n, resolve);
    ws.send(JSON.stringify({ id: n, method, params }));
  });

if (cmd === "eval") {
  const res = await send("Runtime.evaluate", {
    expression: `(async () => { return (${arg}); })()`,
    awaitPromise: true,
    returnByValue: true,
  });
  const r = res.result;
  if (r?.exceptionDetails) console.log("EXCEPTION", JSON.stringify(r.exceptionDetails.exception?.description ?? r.exceptionDetails));
  else console.log(JSON.stringify(r?.result?.value, null, 1));
} else if (cmd === "shot") {
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(arg, Buffer.from(res.result.data, "base64"));
  console.log("saved", arg);
}
ws.close();
process.exit(0);
