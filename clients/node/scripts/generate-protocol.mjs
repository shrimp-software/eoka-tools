import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { generateProtocol, stableJson } from "./schema-to-typescript.mjs";

const packageRoot = resolve(import.meta.dirname, "..");
const repositoryRoot = resolve(packageRoot, "../..");
const args = process.argv.slice(2);
let schemaPath;
let check = false;
for (let index = 0; index < args.length; index += 1) {
  if (args[index] === "--check") check = true;
  else if (args[index] === "--schema" && args[index + 1]) schemaPath = resolve(args[++index]);
  else throw new Error(`Unknown or incomplete argument: ${args[index]}`);
}

let input;
if (schemaPath) input = await readFile(schemaPath, "utf8");
else {
  const exportResult = spawnSync("cargo", ["run", "--quiet", "--locked", "-p", "eoka-protocol", "--bin", "export-sidecar-schema"], {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  if (exportResult.error) throw exportResult.error;
  if (exportResult.status !== 0) throw new Error(`Schema export exited with status ${exportResult.status}`);
  input = exportResult.stdout;
}
const bundle = JSON.parse(input);
const artifacts = [
  ["src/generated/protocol.ts", generateProtocol(bundle)],
  ["src/generated/schema-bundle.json", `${JSON.stringify(stableJson(bundle), null, 2)}\n`],
];
for (const [relative, content] of artifacts) {
  const path = resolve(packageRoot, relative);
  if (check) {
    let existing;
    try {
      existing = await readFile(path, "utf8");
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      throw new Error(`Missing generated artifact: ${relative}; run npm run generate`);
    }
    if (existing !== content) throw new Error(`Stale generated artifact: ${relative}; run npm run generate`);
  } else {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, content);
  }
}
