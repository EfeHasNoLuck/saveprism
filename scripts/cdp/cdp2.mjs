// Extended CDP helper. Commands:
// eval "<expr>" | shot out.png | size W H | clearsize | hover X Y | click X Y | key KeyName | type "text" | full out.png
import { writeFileSync, readFileSync } from "node:fs";
const [cmd, ...args] = process.argv.slice(2);
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
};
await new Promise((r) => (ws.onopen = r));
const send = (method, params = {}) => new Promise((resolve) => { const n = ++id; pending.set(n, resolve); ws.send(JSON.stringify({ id: n, method, params })); });
if (cmd === "eval" || cmd === "evalf") {
  const code = cmd === "evalf" ? readFileSync(args[0], "utf8") : args[0];
  const res = await send("Runtime.evaluate", { expression: `(async () => { return (${code}); })()`, awaitPromise: true, returnByValue: true });
  const r = res.result;
  if (r?.exceptionDetails) console.log("EXCEPTION", JSON.stringify(r.exceptionDetails.exception?.description ?? r.exceptionDetails));
  else console.log(JSON.stringify(r?.result?.value, null, 1));
} else if (cmd === "shot") {
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(args[0], Buffer.from(res.result.data, "base64")); console.log("saved", args[0]);
} else if (cmd === "clip") {
  const [out, x, y, w, h, sc] = args;
  const res = await send("Page.captureScreenshot", { format: "png", clip: { x: +x, y: +y, width: +w, height: +h, scale: +(sc ?? 3) } });
  writeFileSync(out, Buffer.from(res.result.data, "base64")); console.log("saved", out);
} else if (cmd === "sizeshot") {
  await send("Emulation.setDeviceMetricsOverride", { width: +args[0], height: +args[1], deviceScaleFactor: 1, mobile: false });
  await new Promise((r) => setTimeout(r, 1200));
  const info = await send("Runtime.evaluate", { expression: "JSON.stringify([innerWidth, innerHeight, document.documentElement.scrollWidth, document.querySelector('main')?.scrollHeight, document.querySelector('main')?.clientHeight, document.querySelector('main')?.scrollWidth, document.querySelector('main')?.clientWidth])", returnByValue: true });
  console.log(info.result.result.value);
  const res = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(args[2], Buffer.from(res.result.data, "base64")); console.log("saved", args[2]);
  if (args[3]) {
    await send("Runtime.evaluate", { expression: "document.querySelector('main').scrollTop = 99999" });
    await new Promise((r) => setTimeout(r, 300));
    const res2 = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(args[3], Buffer.from(res2.result.data, "base64")); console.log("saved", args[3]);
    await send("Runtime.evaluate", { expression: "document.querySelector('main').scrollTop = 0" });
  }
  await send("Emulation.clearDeviceMetricsOverride");
} else if (cmd === "size") {
  const r = await send("Emulation.setDeviceMetricsOverride", { width: +args[0], height: +args[1], deviceScaleFactor: 1, mobile: false });
  console.log(JSON.stringify(r));
} else if (cmd === "clearsize") {
  console.log(JSON.stringify(await send("Emulation.clearDeviceMetricsOverride")));
} else if (cmd === "hover") {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: +args[0], y: +args[1] }); console.log("hovered");
} else if (cmd === "click") {
  const x = +args[0], y = +args[1];
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }); console.log("clicked");
} else if (cmd === "key") {
  const key = args[0];
  const codes = { Escape: 27, Enter: 13, Tab: 9 };
  await send("Input.dispatchKeyEvent", { type: "keyDown", key, code: key, windowsVirtualKeyCode: codes[key] ?? 0 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key, code: key, windowsVirtualKeyCode: codes[key] ?? 0 }); console.log("key", key);
} else if (cmd === "type") {
  await send("Input.insertText", { text: args[0] }); console.log("typed");
}
ws.close();
process.exit(0);
