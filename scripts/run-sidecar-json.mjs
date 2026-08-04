import { spawnSync } from "node:child_process";

const [executable, tool, argsFile] = process.argv.slice(2);
if (!executable || !tool || !argsFile) {
  process.stderr.write("usage: node run-sidecar-json.mjs <exe> <tool> <args.json>\n");
  process.exit(2);
}

const timeoutMs = Number(process.env.CODE_MEMORY_SMOKE_TIMEOUT_MS ?? "600000");
const maxBufferBytes = Number(
  process.env.CODE_MEMORY_SMOKE_MAX_BUFFER_BYTES ?? 256 * 1024 * 1024,
);

const run = spawnSync(executable, ["cli", tool, "--args-file", argsFile], {
  encoding: "utf8",
  env: process.env,
  maxBuffer: maxBufferBytes,
  timeout: timeoutMs,
  windowsHide: true,
});
process.stdout.write(run.stdout ?? "");
process.stderr.write(run.stderr ?? run.error?.message ?? "");
process.exit(run.status ?? 1);
