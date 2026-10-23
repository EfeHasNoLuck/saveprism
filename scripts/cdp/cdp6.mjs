// node cdp6.mjs <file.js> : evaluates the file with focus emulation on (works while the window is in the background).
import { readFileSync } from "node:fs";
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) => new Promise((res) => { const n = ++id; pending.set(n, res); ws.send(JSON.stringify({ id: n, method, params })); });
await send("Emulation.setFocusEmulationEnabled", { enabled: true });
const code = readFileSync(process.argv[2], "utf8");
const res = await send("Runtime.evaluate", { expression: `(async () => { return (${code}); })()`, awaitPromise: true, returnByValue: true });
const r = res.result;
if (r?.exceptionDetails) console.log("EXCEPTION", JSON.stringify(r.exceptionDetails.exception?.description ?? r.exceptionDetails));
else console.log(JSON.stringify(r?.result?.value));
await send("Emulation.setFocusEmulationEnabled", { enabled: false });
ws.close(); process.exit(0);
