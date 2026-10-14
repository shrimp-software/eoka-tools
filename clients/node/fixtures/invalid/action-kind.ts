import type { MethodParams } from "../../src/generated/protocol.js";
const params: MethodParams["actions.perform"] = {
  pageId: "page-1",
  operation_id: "operation-1",
  action: { kind: "replay_unverified_action" },
};
export { params };
