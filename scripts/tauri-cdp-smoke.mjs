import { readFileSync, writeFileSync } from "node:fs";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 1) {
  const key = process.argv[index];
  const next = process.argv[index + 1];
  if (key.startsWith("--")) {
    args.set(key.slice(2), next && !next.startsWith("--") ? next : "true");
    if (next && !next.startsWith("--")) {
      index += 1;
    }
  }
}

const port = args.get("port") ?? "9222";
const width = Number(args.get("width") ?? "1440");
const height = Number(args.get("height") ?? "900");
const expression = args.has("eval-file")
  ? readFileSync(args.get("eval-file"), "utf8")
  : args.get("eval") ??
    "({title:document.title,bodyLen:document.body.innerText.length,body:document.body.innerText.slice(0,1000),rootHtmlLen:document.getElementById('root')?.innerHTML.length??-1})";
const screenshotOut = args.get("screenshot");

const targets = await fetch(`http://127.0.0.1:${port}/json`).then((response) => {
  if (!response.ok) {
    throw new Error(`CDP target list failed: ${response.status}`);
  }
  return response.json();
});
const page = targets.find((target) => target.type === "page") ?? targets[0];
if (!page?.webSocketDebuggerUrl) {
  throw new Error("No CDP page target found");
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let commandId = 0;
const pending = new Map();

function send(method, params = {}) {
  const id = ++commandId;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (!message.id || !pending.has(message.id)) {
    return;
  }
  const handler = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) {
    handler.reject(new Error(`${message.error.message}: ${message.error.data ?? ""}`));
  } else {
    handler.resolve(message);
  }
});

await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});

await send("Runtime.enable");
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", {
  width,
  height,
  deviceScaleFactor: 1,
  mobile: false,
});

const evaluated = await send("Runtime.evaluate", {
  expression,
  awaitPromise: true,
  returnByValue: true,
});

if (evaluated.result.exceptionDetails) {
  const details = evaluated.result.exceptionDetails;
  throw new Error(
    `${details.exception?.description ?? details.text ?? "CDP evaluation failed"}` +
      (Number.isInteger(details.lineNumber) ? ` at ${details.lineNumber + 1}:${(details.columnNumber ?? 0) + 1}` : ""),
  );
}

let screenshot = null;
if (screenshotOut && screenshotOut !== "true") {
  await send("Page.bringToFront");
  await send("Runtime.evaluate", {
    expression: "new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 2500))))",
    awaitPromise: true,
  });
  const captured = await send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  writeFileSync(screenshotOut, Buffer.from(captured.result.data, "base64"));
  screenshot = screenshotOut;
}

socket.close();

console.log(
  JSON.stringify(
    {
      target: page.url,
      value: evaluated.result.result.value ?? null,
      screenshot,
    },
    null,
    2,
  ),
);
