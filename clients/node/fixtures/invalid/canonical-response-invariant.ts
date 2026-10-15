import type { Response } from "../../src/generated/protocol.js";

const response: Response = {
  id: 1,
  result: null,
  error: { code: "Internal", message: "failure" },
};

void response;
