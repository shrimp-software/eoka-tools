package eoka

import (
	"context"
	"encoding/json"
)

type AgentObserveOptions struct {
	Cursor      string `json:"cursor,omitempty"`
	Role        string `json:"role,omitempty"`
	Name        string `json:"name,omitempty"`
	MaxControls int    `json:"max_controls,omitempty"`
	TimeoutMS   uint64 `json:"timeout_ms,omitempty"`
}

type AgentStepRequest struct {
	Destination     string         `json:"destination,omitempty"`
	OperationID     string         `json:"operation_id,omitempty"`
	Action          map[string]any `json:"action"`
	Mode            string         `json:"mode,omitempty"`
	Expect          map[string]any `json:"expect,omitempty"`
	TimeoutMS       uint64         `json:"timeout_ms,omitempty"`
	ActionTimeoutMS uint64         `json:"action_timeout_ms,omitempty"`
	MaxControls     int            `json:"max_controls,omitempty"`
}

type AgentControl struct {
	Focused           bool    `json:"focused"`
	Editable          bool    `json:"editable"`
	Readonly          bool    `json:"readonly"`
	Required          bool    `json:"required"`
	Invalid           bool    `json:"invalid"`
	ValidationMessage *string `json:"validation_message"`
	Target            string  `json:"target"`
	Role              string  `json:"role"`
	Name              string  `json:"name"`
	Disabled          bool    `json:"disabled"`
	Checked           *bool   `json:"checked"`
	Expanded          *bool   `json:"expanded"`
}

type AgentObservation struct {
	NextCursor    *string           `json:"next_cursor"`
	TotalControls int               `json:"total_controls"`
	ObservationID string            `json:"observation_id"`
	TabID         string            `json:"tab_id"`
	Page          json.RawMessage   `json:"page"`
	Tabs          []json.RawMessage `json:"tabs"`
	Controls      []AgentControl    `json:"controls"`
	Truncated     bool              `json:"truncated"`
	Warnings      []string          `json:"warnings"`
}

type AgentStepResult struct {
	Status           string            `json:"status"`
	OperationID      string            `json:"operation_id"`
	Operation        *OperationRecord  `json:"operation"`
	OperationError   *ErrorDetail      `json:"operation_error"`
	Observation      *AgentObservation `json:"observation"`
	ObservationError *string           `json:"observation_error"`
}

func (p *Page) AgentObserve(ctx context.Context, options AgentObserveOptions) (AgentObservation, error) {
	var result AgentObservation
	params, err := json.Marshal(options)
	if err != nil {
		return result, err
	}
	var input map[string]any
	if err := json.Unmarshal(params, &input); err != nil {
		return result, err
	}
	err = p.call(ctx, "agent.observe", input, &result)
	return result, err
}

func (p *Page) AgentStep(ctx context.Context, request AgentStepRequest) (AgentStepResult, error) {
	var result AgentStepResult
	if request.OperationID == "" {
		id, err := NewOperationID()
		if err != nil {
			return result, err
		}
		request.OperationID = id
	}
	result.OperationID = request.OperationID
	if err := p.b.t.requireCapability("durableActions"); err != nil {
		return result, operationError(request.OperationID, OperationRecord{}, err)
	}
	if err := ctx.Err(); err != nil {
		return result, &OperationError{OperationID: request.OperationID, Err: err}
	}
	params, err := json.Marshal(request)
	if err != nil {
		return result, err
	}
	var input map[string]any
	if err := json.Unmarshal(params, &input); err != nil {
		return result, err
	}
	if err := p.call(ctx, "agent.step", input, &result); err != nil {
		return result, operationError(request.OperationID, OperationRecord{}, err)
	}
	return result, nil
}
