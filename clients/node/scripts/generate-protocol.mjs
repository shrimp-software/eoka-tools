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

// Catalog operations: one entry per operation the server projects from the
// operation catalog. Both clients get named access to all of them from here,
// so a new catalog operation reaches Node and Go without hand-written code.
const catalog = [...bundle.catalogMethods].sort();
const pascal = method => method.split(/[._]/).map(part => part[0].toUpperCase() + part.slice(1)).join("");
const camel = method => pascal(method).replace(/^./, first => first.toLowerCase());
const needsInput = method => (bundle.methods[method].params.required ?? []).some(name => name !== "pageId");
const header = "// Generated from the eoka operation catalog by scripts/generate-protocol.mjs. Do not edit.";

const nodeCatalog = `${header}
import type { CallOptions } from "../client.js";
import type { MethodParams, MethodResults } from "./protocol.js";

export const catalogMethods = ${JSON.stringify(catalog)} as const;
export type CatalogMethod = (typeof catalogMethods)[number];
/** An operation's input without the page ID, which the page supplies. */
export type CatalogInput<M extends CatalogMethod> = Omit<MethodParams[M], "pageId">;

export interface CatalogPage {
  readonly id: string;
  invoke<M extends CatalogMethod>(method: M, params: MethodParams[M], options?: CallOptions): Promise<MethodResults[M]>;
}

/** Every catalog operation as a method bound to one page. */
export function catalogOperations(page: CatalogPage) {
  const call = <M extends CatalogMethod>(method: M, input: CatalogInput<M>, options?: CallOptions) =>
    page.invoke(method, { ...input, pageId: page.id } as MethodParams[M], options);
  return {
${catalog.map(method => `    ${camel(method)}: (input: CatalogInput<${JSON.stringify(method)}>${needsInput(method) ? "" : " = {}"}, options?: CallOptions) => call(${JSON.stringify(method)}, input, options),`).join("\n")}
  };
}

export type CatalogOperations = ReturnType<typeof catalogOperations>;
`;

// gofmt aligns the `=` of a const block on the longest name.
const goWidth = Math.max(...catalog.map(method => pascal(method).length)) + 2;
const goCatalog = `${header}

package eoka

// Catalog operation method names for Page.Invoke.
const (
${catalog.map(method => `\t${`Op${pascal(method)}`.padEnd(goWidth)} = ${JSON.stringify(method)}`).join("\n")}
)

// CatalogMethods lists every catalog operation this client was generated with.
var CatalogMethods = []string{
${catalog.map(method => `\tOp${pascal(method)},`).join("\n")}
}
`;

const artifacts = [
  ["src/generated/protocol.ts", generateProtocol(bundle)],
  ["src/generated/schema-bundle.json", `${JSON.stringify(stableJson(bundle), null, 2)}\n`],
  ["src/generated/catalog.ts", nodeCatalog],
  ["../go/catalog_generated.go", goCatalog],
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
