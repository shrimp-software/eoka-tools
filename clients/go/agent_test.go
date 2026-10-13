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

func TestAgentPaginationSurvivesRPCProjection(t *testing.T) {
	page := newFakePage(t, func(id int64, method string, params json.RawMessage) (any, *rpcError) {
		var input map[string]any
		if err := json.Unmarshal(params, &input); err != nil {
			t.Fatal(err)
		}
		if input["offset"] != float64(200) {
			t.Fatalf("missing continuation: %s", params)
		}
		view := map[string]any{"controls": []any{}, "next_offset": 240, "truncated": true}
		if method == "agent.observe" {
			return view, nil
		}
		if method != "agent.step" {
			t.Fatalf("unexpected method %s", method)
		}
		return map[string]any{"operation_id": input["operation_id"], "observation": view}, nil
	})
	view, err := page.AgentObserve(context.Background(), AgentObserveOptions{Offset: 200})
	if err != nil || view.NextOffset == nil || *view.NextOffset != 240 {
		t.Fatalf("lost observation continuation: %#v %v", view, err)
	}
	step, err := page.AgentStep(context.Background(), AgentStepRequest{Action: map[string]any{"kind": "press_key", "key": "Tab"}, Offset: 200})
	if err != nil || step.Observation == nil || step.Observation.NextOffset == nil || *step.Observation.NextOffset != 240 {
		t.Fatalf("lost step continuation: %#v %v", step, err)
	}
}
