import { readdir } from "node:fs/promises";
import { resolve } from "node:path";
import ts from "typescript";

const root = resolve(import.meta.dirname, "..");
const configPath = resolve(root, "tsconfig.json");
const config = ts.readConfigFile(configPath, ts.sys.readFile);
const formatter = {
  getCanonicalFileName: (path) => path,
  getCurrentDirectory: () => root,
  getNewLine: () => "\n",
};
function reject(diagnostics) {
  throw new Error(ts.formatDiagnosticsWithColorAndContext(diagnostics, formatter));
}
if (config.error) reject([config.error]);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root, undefined, configPath);
if (parsed.errors.length) reject(parsed.errors);
const valid = ts.createProgram(parsed.fileNames, parsed.options);
const validDiagnostics = ts.getPreEmitDiagnostics(valid);
if (validDiagnostics.length) reject(validDiagnostics);

const invalidRoot = resolve(root, "fixtures/invalid");
const fixtures = (await readdir(invalidRoot)).filter((file) => file.endsWith(".ts")).sort();
if (fixtures.length === 0) throw new Error("No rejected TypeScript consumers found");
for (const fixture of fixtures) {
  const path = resolve(invalidRoot, fixture);
  const program = ts.createProgram([path], parsed.options);
  const syntax = program.getSyntacticDiagnostics();
  if (syntax.length) reject(syntax);
  const diagnostics = ts.getPreEmitDiagnostics(program);
  const expected = diagnostics.filter((diagnostic) => diagnostic.file?.fileName === path && diagnostic.category === ts.DiagnosticCategory.Error);
  const unexpected = diagnostics.filter((diagnostic) => !expected.includes(diagnostic));
  if (unexpected.length) reject(unexpected);
  if (expected.length === 0) throw new Error(`Invalid consumer unexpectedly typechecked: ${fixture}`);
}
console.log(`Valid consumers typechecked; ${fixtures.length} invalid consumers rejected.`);
