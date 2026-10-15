import childProcess from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import net from "node:net";
import { syncBuiltinESMExports } from "node:module";

for (const [module, names] of [
  [childProcess, ["spawn", "spawnSync", "exec", "execSync", "execFile", "execFileSync", "fork"]],
  [fs, ["writeFile", "writeFileSync", "appendFile", "appendFileSync", "mkdir", "mkdirSync", "createWriteStream", "rm", "rmSync", "unlink", "unlinkSync", "rename", "renameSync"]],
  [fs.promises, ["writeFile", "appendFile", "mkdir", "rm", "unlink", "rename", "open"]],
  [http, ["request", "get"]],
  [https, ["request", "get"]],
  [net, ["connect", "createConnection"]],
]) {
  for (const name of names) {
    module[name] = () => { throw new Error(`Import attempted ${name}`); };
  }
}
globalThis.fetch = () => { throw new Error("Import attempted fetch"); };
syncBuiltinESMExports();
const sdk = await import("@cbxss/eoka");
const protocol = await import("@cbxss/eoka/protocol");
if (typeof sdk.Browser !== "function" || typeof sdk.Recording !== "function" || typeof sdk.RecordingError !== "function" || typeof sdk.Page.prototype.screenshotToFile !== "function" || typeof sdk.Page.prototype.record !== "function" || typeof sdk.Browser.prototype.recording !== "function" || typeof sdk.installBrowser !== "function" || typeof sdk.InstallationError !== "function" || typeof sdk.detectPlatform !== "function" || !sdk.releaseManifest || sdk.protocolVersion.major !== protocol.protocolVersion.major) {
  throw new Error("Package exports did not load");
}
