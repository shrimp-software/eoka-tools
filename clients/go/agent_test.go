package eoka

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestAgentStepRetainsGeneratedIdentityOnRPCFailure(t *testing.T) {
	var admitted string
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		if method != "agent.step" {
			t.Fatalf("unexpected method %s", method)
		}
		var request AgentStepRequest
		if err := json.Unmarshal(params, &request); err != nil {
			t.Fatal(err)
		}
		admitted = request.OperationID
		return nil, &rpcError{Code: ErrCodeUnknownMethod, Message: "unavailable"}
	})
	result, err := page.AgentStep(context.Background(), AgentStepRequest{Action: map[string]any{"kind": "press_key", "key": "Tab"}})
	if len(admitted) != 36 || result.OperationID != admitted || err == nil || !strings.Contains(err.Error(), admitted) {
		t.Fatalf("missing recovery identity: %q %#v %v", admitted, result, err)
	}
}
