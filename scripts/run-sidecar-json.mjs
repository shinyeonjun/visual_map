import { spawnSync } from "node:child_process";

const [executable, tool, argsFile] = process.argv.slice(2);
if (!executable || !tool || !argsFile) {
  process.stderr.write("usage: node run-sidecar-json.mjs <exe> <tool> <args.json>\n");
  process.exit(2);
}

const run = spawnSync(executable, ["cli", tool, "--args-file", argsFile], {
  encoding: "utf8",
  env: process.env,
  maxBuffer: 32 * 1024 * 1024,
  timeout: 120_000,
  windowsHide: true,
});
process.stdout.write(run.stdout ?? "");
process.stderr.write(run.stderr ?? run.error?.message ?? "");
process.exit(run.status ?? 1);
