const annotations = new Set([
  "$schema", "$id", "$comment", "title", "description", "default", "examples",
  "deprecated", "readOnly", "writeOnly", "format", "contentEncoding", "contentMediaType",
  "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "multipleOf",
  "minLength", "maxLength", "pattern", "minItems", "maxItems", "uniqueItems",
  "minProperties", "maxProperties",
]);
const structural = new Set([
  "$ref", "$defs", "definitions", "type", "enum", "const", "anyOf", "oneOf",
  "allOf", "properties", "required", "additionalProperties", "items", "prefixItems",
]);
const identifier = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
const own = (value, key) => Object.hasOwn(value, key);
const compareEntries = ([a], [b]) => a < b ? -1 : a > b ? 1 : 0;
const sortedEntries = (value) => Object.entries(value).sort(compareEntries);

export function stableJson(value) {
  if (Array.isArray(value)) return value.map(stableJson);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(sortedEntries(value).map(([key, item]) => [key, stableJson(item)]));
  }
  return value;
}

function literal(value) {
  if (value === null || ["string", "boolean", "number"].includes(typeof value)) {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(literal).join(", ")}]`;
  if (typeof value === "object") {
    return `{ ${sortedEntries(value).map(([key, item]) => `${JSON.stringify(key)}: ${literal(item)};`).join(" ")} }`;
  }
  throw new Error("Unsupported JSON literal");
}

function union(parts) {
  return parts.length === 1 ? parts[0] : `(${parts.join(" | ")})`;
}

export function schemaDeclarations(root, name) {
  if (!identifier.test(name)) throw new Error(`Invalid declaration name: ${name}`);
  const declarations = [];
  const definitions = new Map();
  const symbols = new Set([name]);
  for (const namespace of ["$defs", "definitions"]) {
    const entries = root?.[namespace];
    if (entries === undefined) continue;
    if (entries === null || typeof entries !== "object" || Array.isArray(entries)) {
      throw new Error(`${name}.${namespace}: expected schema definitions`);
    }
    for (const [key, schema] of sortedEntries(entries)) {
      const suffix = key.replace(/[^A-Za-z0-9_$]/g, (character) => `_u${character.codePointAt(0).toString(16)}_`);
      const symbol = `${name}__${suffix}`;
      if (symbols.has(symbol)) throw new Error(`${name}: definition name collision at ${key}`);
      symbols.add(symbol);
      definitions.set(`#/${namespace}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`, { symbol, schema });
    }
  }

  function convert(schema, path) {
    if (schema === true) return "unknown";
    if (schema === false) return "never";
    if (schema === null || typeof schema !== "object" || Array.isArray(schema)) {
      throw new Error(`${path}: expected a JSON Schema object or boolean`);
    }
    for (const keyword of Object.keys(schema)) {
      if (!annotations.has(keyword) && !structural.has(keyword)) {
        throw new Error(`${path}: unsupported JSON Schema keyword ${keyword}`);
      }
    }
    if (path !== name && (own(schema, "$defs") || own(schema, "definitions"))) {
      throw new Error(`${path}: nested schema definitions are unsupported`);
    }
    const parts = [];
    if (own(schema, "$ref")) {
      if (schema.$ref === "#") parts.push(name);
      else {
        const definition = definitions.get(schema.$ref);
        if (!definition) throw new Error(`${path}: unresolved or unsupported reference ${schema.$ref}`);
        parts.push(definition.symbol);
      }
    }
    if (own(schema, "const")) parts.push(literal(schema.const));
    if (own(schema, "enum")) {
      if (!Array.isArray(schema.enum) || schema.enum.length === 0) throw new Error(`${path}: invalid enum`);
      parts.push(union(schema.enum.map(literal)));
    }
    for (const key of ["anyOf", "oneOf", "allOf"]) {
      if (!own(schema, key)) continue;
      if (!Array.isArray(schema[key]) || schema[key].length === 0) throw new Error(`${path}: invalid ${key}`);
      const values = schema[key].map((item, index) => convert(item, `${path}.${key}[${index}]`));
      if (key !== "allOf" && schema[key].every((item) => item?.type === "object" && item.additionalProperties === false)) {
        const keys = [...new Set(schema[key].flatMap((item) => Object.keys(item.properties ?? {})))];
        for (let index = 0; index < values.length; index += 1) {
          const properties = schema[key][index].properties ?? {};
          const excluded = keys.filter((field) => !own(properties, field));
          if (excluded.length > 0) {
            values[index] = `(${values[index]} & { ${excluded.map((field) => `${JSON.stringify(field)}?: never;`).join(" ")} })`;
          }
        }
      }
      parts.push(key === "allOf" ? `(${values.join(" & ")})` : union(values));
    }
    if (Array.isArray(schema.type)) {
      if (schema.type.length === 0) throw new Error(`${path}: empty type union`);
      parts.push(union(schema.type.map((type) => primitive({ ...schema, type }, path))));
    } else if (own(schema, "type")) {
      parts.push(primitive(schema, path));
    } else if (own(schema, "properties") || own(schema, "additionalProperties") || own(schema, "required")) {
      parts.push(object(schema, path));
    } else if (own(schema, "items") || own(schema, "prefixItems")) {
      parts.push(array(schema, path));
    }
    return parts.length === 0 ? "unknown" : parts.length === 1 ? parts[0] : `(${parts.join(" & ")})`;
  }

  function primitive(schema, path) {
    switch (schema.type) {
      case "null": return "null";
      case "boolean": return "boolean";
      case "integer":
      case "number": return "number";
      case "string": return "string";
      case "object": return object(schema, path);
      case "array": return array(schema, path);
      default: throw new Error(`${path}: unsupported schema type ${JSON.stringify(schema.type)}`);
    }
  }

  function object(schema, path) {
    const properties = schema.properties ?? {};
    if (properties === null || typeof properties !== "object" || Array.isArray(properties)) {
      throw new Error(`${path}: invalid properties`);
    }
    const required = schema.required ?? [];
    if (!Array.isArray(required) || required.some((key) => typeof key !== "string")) {
      throw new Error(`${path}: invalid required properties`);
    }
    const requiredKeys = new Set(required);
    const fields = new Map(Object.entries(properties));
    for (const key of requiredKeys) if (!fields.has(key)) fields.set(key, true);
    const additional = schema.additionalProperties ?? true;
    const extra = convert(additional, `${path}.additionalProperties`);
    if (fields.size > 0 && additional !== true && additional !== false) {
      throw new Error(`${path}: typed additionalProperties alongside named properties cannot be represented faithfully`);
    }
    if (fields.size === 0) return `Record<string, ${extra}>`;
    const rows = [...fields].sort(compareEntries).map(([key, value]) => {
      const optional = requiredKeys.has(key) ? "" : "?";
      return `${JSON.stringify(key)}${optional}: ${convert(value, `${path}.properties.${key}`)};`;
    });
    if (additional !== false) rows.push(`[key: string]: ${extra};`);
    return `{ ${rows.join(" ")} }`;
  }

  function array(schema, path) {
    if (Array.isArray(schema.items)) throw new Error(`${path}: draft-07 tuple items are unsupported; use prefixItems`);
    const item = convert(schema.items ?? true, `${path}.items`);
    if (!own(schema, "prefixItems")) return `Array<${item}>`;
    if (!Array.isArray(schema.prefixItems)) throw new Error(`${path}: invalid prefixItems`);
    const minimum = schema.minItems ?? 0;
    const maximum = schema.maxItems ?? Infinity;
    if (!Number.isSafeInteger(minimum) || minimum < 0 || maximum < minimum) throw new Error(`${path}: invalid tuple bounds`);
    if (minimum > schema.prefixItems.length) throw new Error(`${path}: tuple minItems exceeds prefix length`);
    const prefix = schema.prefixItems.slice(0, maximum).map((entry, index) => {
      const type = convert(entry, `${path}.prefixItems[${index}]`);
      return index < minimum ? type : `(${type})?`;
    });
    if (schema.items !== false) {
      if (Number.isFinite(maximum) && maximum > prefix.length) {
        throw new Error(`${path}: bounded tuple tail is unsupported`);
      }
      if (!Number.isFinite(maximum)) prefix.push(`...Array<${item}>`);
    }
    return `[${prefix.join(", ")}]`;
  }

  declarations.push(`export type ${name} = ${convert(root, name)};`);
  for (const { symbol, schema } of definitions.values()) {
    declarations.push(`export type ${symbol} = ${convert(schema, `${name}.${symbol}`)};`);
  }
  return declarations;
}

function methodName(method) {
  return method.split(/[^A-Za-z0-9]+/).map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join("");
}

export function generateProtocol(bundle) {
  if (!bundle || typeof bundle !== "object" || !bundle.methods || !bundle.types || !bundle.protocolVersion) {
    throw new Error("Expected canonical schema_bundle with protocolVersion, methods and types");
  }
  const lines = [];
  const symbols = new Set(["MethodParams", "MethodResults", "MethodName", "ProtocolRequest", "ProtocolSuccessResponse", "ProtocolErrorResponse", "ProtocolResponse", "protocolVersion", "methodNames"]);
  function emit(schema, name) {
    if (symbols.has(name)) throw new Error(`Duplicate declaration name: ${name}`);
    for (const line of schemaDeclarations(schema, name)) {
      const symbol = line.match(/^export type ([A-Za-z0-9_$]+)/)[1];
      if (symbols.has(symbol)) throw new Error(`Duplicate declaration name: ${symbol}`);
      symbols.add(symbol);
      lines.push(line);
    }
    lines.push("");
  }
  for (const [name, schema] of sortedEntries(bundle.types)) emit(schema, name);
  const methods = sortedEntries(bundle.methods);
  if (methods.length === 0) throw new Error("The sidecar method registry is empty");
  const entries = methods.map(([method, schemas]) => {
    if (!schemas || !own(schemas, "params") || !own(schemas, "result")) throw new Error(`Missing method schemas: ${method}`);
    const name = methodName(method);
    const params = `Method${name}Params`;
    const result = `Method${name}Result`;
    emit(schemas.params, params);
    emit(schemas.result, result);
    return { method, params, result };
  });
  if (!symbols.has("ServerError")) throw new Error("Canonical types must include ServerError");
  lines.push("export type MethodParams = {");
  for (const { method, params } of entries) lines.push(`  ${JSON.stringify(method)}: ${params};`);
  lines.push("};", "", "export type MethodResults = {");
  for (const { method, result } of entries) lines.push(`  ${JSON.stringify(method)}: ${result};`);
  lines.push("};", "", "export type MethodName = keyof MethodParams;", "");
  lines.push("export type ProtocolRequest<M extends MethodName = MethodName> = {", "  [K in M]: { id: number; method: K; params: MethodParams[K] };", "}[M];", "");
  lines.push("export type ProtocolSuccessResponse<M extends MethodName = MethodName> = {", "  [K in M]: { id: number; result: MethodResults[K]; error?: never };", "}[M];", "");
  lines.push("export type ProtocolErrorResponse = { id: number | null; error: ServerError; result?: never };", "", "export type ProtocolResponse<M extends MethodName = MethodName> = ProtocolSuccessResponse<M> | ProtocolErrorResponse;", "");
  lines.push(`export const protocolVersion = ${JSON.stringify(stableJson(bundle.protocolVersion))} as const;`);
  lines.push(`export const methodNames = ${JSON.stringify(entries.map(({ method }) => method))} as const;`, "");
  return lines.join("\n");
}
