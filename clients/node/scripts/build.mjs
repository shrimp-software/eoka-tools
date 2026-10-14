import { execFile } from "node:child_process";
import { copyFile, mkdir, rm } from "node:fs/promises";
import { resolve } from "node:path";
import { promisify } from "node:util";

const root = resolve(import.meta.dirname, "..");
await rm(new URL("../dist/", import.meta.url), { recursive: true, force: true });
try {
  await promisify(execFile)(process.execPath, [
    resolve(root, "node_modules/typescript/lib/tsc.js"),
    "-p",
    "tsconfig.build.json",
  ], { cwd: root });
} catch (error) {
  if (error.stdout) process.stderr.write(error.stdout);
  if (error.stderr) process.stderr.write(error.stderr);
  throw error;
}
await mkdir(new URL("../dist/generated/", import.meta.url), { recursive: true });
await copyFile(
  new URL("../src/generated/schema-bundle.json", import.meta.url),
  new URL("../dist/generated/schema-bundle.json", import.meta.url),
);
await copyFile(
  new URL("../release-manifest.json", import.meta.url),
  new URL("../dist/release-manifest.json", import.meta.url),
);
await copyFile(
  new URL("../../../LICENSE", import.meta.url),
  new URL("../dist/LICENSE", import.meta.url),
);
