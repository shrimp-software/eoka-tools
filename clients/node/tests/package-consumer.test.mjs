import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { detectPlatform, releaseManifest } from "../dist/index.js";

const exec = promisify(execFile);
const root = fileURLToPath(new URL("../", import.meta.url));
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

test("packed SDK installs without scripts, imports silently, and runs an external typed consumer", { timeout: 120000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "eoka-package-consumer-"));
  try {
    const packed = await exec(npm, ["pack", "--ignore-scripts", "--json", "--pack-destination", directory], { cwd: root });
    const [archive] = JSON.parse(packed.stdout);
    assert.equal(archive.version, releaseManifest.sdkVersion);
    assert.ok(archive.files.every((file) => file.path.startsWith("dist/") || file.path.startsWith("examples/") || ["package.json", "README.md", "LICENSE"].includes(file.path)));
    const consumer = join(directory, "consumer");
    const home = join(directory, "home");
    await mkdir(consumer);
    await mkdir(home);
    for (const file of ["package.json", "tsconfig.json", "import-probe.mjs"]) {
      await copyFile(new URL(`../fixtures/package-consumer/${file}`, import.meta.url), join(consumer, file));
    }
    await copyFile(new URL("../fixtures/valid/sdk-consumer.ts", import.meta.url), join(consumer, "consumer.ts"));
    await exec(npm, ["install", "--ignore-scripts", "--omit=optional", "--no-audit", "--no-fund", join(directory, archive.filename)], { cwd: consumer });
    const importEnvironment = { ...process.env, HOME: home, USERPROFILE: home, XDG_CACHE_HOME: join(home, "cache") };
    delete importEnvironment.EOKA_SERVER_BIN;
    delete importEnvironment.EOKA_CHROME_PATH;
    const imported = await exec(process.execPath, ["import-probe.mjs"], { cwd: consumer, env: importEnvironment });
    assert.equal(imported.stdout, "");
    assert.equal(imported.stderr, "");
    const missingServer = await exec(process.execPath, [
      "--input-type=module",
      "--eval",
      'const {Browser,InstallationError}=await import("@cbxss/eoka"); try { await Browser.launch({download:"never"}); throw new Error("Missing platform package was accepted"); } catch(error) { if (!(error instanceof InstallationError) || error.code !== "ServerPackageMissing") throw error; }',
    ], { cwd: consumer, env: importEnvironment });
    assert.equal(missingServer.stdout, "");
    assert.equal(missingServer.stderr, "");
    assert.deepEqual(await readdir(home), []);
    await exec(process.execPath, [
      join(root, "node_modules/typescript/lib/tsc.js"),
      "-p", "tsconfig.json",
      "--typeRoots", join(root, "node_modules/@types"),
    ], { cwd: consumer });
    const ran = await exec(process.execPath, [
      "--input-type=module",
      "--eval",
      'const {runConsumer} = await import("./output/consumer.js"); console.log(JSON.stringify(runConsumer()));',
    ], { cwd: consumer });
    const result = JSON.parse(ran.stdout);
    assert.equal(result.error.operationId, "7f2e1450-f1bd-4a5e-b111-215a5d873e26");
    assert.equal(ran.stderr, "");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("locally staged SDK and native package capture images and native artifacts offline outside the repository", {
  timeout: 120000,
  skip: !process.env.EOKA_RELEASE_STAGE_DIR || !process.env.EOKA_BROWSER_CACHE_DIR,
}, async () => {
  const stage = process.env.EOKA_RELEASE_STAGE_DIR;
  const platform = detectPlatform();
  const directory = await mkdtemp(join(tmpdir(), "e4-consumer-"));
  try {
    const archives = [];
    for (const source of [join(stage, "platforms", platform), join(stage, "sdk")]) {
      const packed = await exec(npm, ["pack", "--ignore-scripts", "--json", "--pack-destination", directory], { cwd: source });
      const [archive] = JSON.parse(packed.stdout);
      archives.push(join(directory, archive.filename));
    }
    const consumer = join(directory, "consumer");
    await mkdir(consumer);
    await copyFile(new URL("../fixtures/package-consumer/package.json", import.meta.url), join(consumer, "package.json"));
    await copyFile(new URL("../fixtures/package-consumer/import-probe.mjs", import.meta.url), join(consumer, "import-probe.mjs"));
    await copyFile(new URL("../fixtures/package-consumer/native-capture.mjs", import.meta.url), join(consumer, "native-capture.mjs"));
    await exec(npm, ["install", "--ignore-scripts", "--omit=optional", "--no-audit", "--no-fund", ...archives], { cwd: consumer });
    const env = { ...process.env, EOKA_RUNTIME_DIR: join(directory, "r"), EOKA_STATE_DIR: join(directory, "state") };
    delete env.EOKA_SERVER_BIN;
    delete env.EOKA_CHROME_PATH;
    const imported = await exec(process.execPath, ["import-probe.mjs"], { cwd: consumer, env });
    assert.equal(imported.stdout, "");
    assert.equal(imported.stderr, "");
    const launched = await exec(process.execPath, [
      "native-capture.mjs",
    ], { cwd: consumer, env, timeout: 110000 });
    const result = JSON.parse(launched.stdout);
    assert.equal(result.title, "offline-staged");
    assert.equal(result.ownership, "owned");
    assert.ok(result.browserVersion.includes(result.buildId));
    assert.equal(launched.stderr, "");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
