import Ajv2020 from "ajv/dist/2020.js";

export function decodeMethodResult(bundle, method, serializedResponse) {
  const contract = bundle.methods[method];
  if (!contract) throw new Error(`Unknown sidecar method: ${method}`);
  const response = JSON.parse(serializedResponse);
  if (response === null || typeof response !== "object" || Array.isArray(response)
      || !Number.isSafeInteger(response.id) || response.id <= 0
      || !Object.hasOwn(response, "result") || Object.hasOwn(response, "error")) {
    throw new Error("Expected a successful sidecar response with a safe positive request ID");
  }
  const ajv = new Ajv2020({ strict: false, validateFormats: false });
  const validate = ajv.compile(contract.result);
  if (!validate(response.result)) {
    throw new Error(`Invalid ${method} result: ${ajv.errorsText(validate.errors)}`);
  }
  return response.result;
}
