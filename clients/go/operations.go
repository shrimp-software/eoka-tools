package eoka

import (
	"context"
	"crypto/rand"
	"encoding/json"
	"errors"
	"fmt"
)

type ActionOptions struct {
	TimeoutMS uint64         `json:"timeout_ms,omitempty"`
	Expect    map[string]any `json:"expect,omitempty"`
}

type OperationRecord struct {
	ID                string          `json:"id"`
	RuntimeID         string          `json:"runtime_id"`
	TargetID          string          `json:"target_id"`
	State             string          `json:"state"`
	Outcome           string          `json:"outcome"`
	DispatchIntent    bool            `json:"dispatch_intent"`
	PreparationIntent bool            `json:"preparation_intent"`
	Receipt           json.RawMessage `json:"receipt"`
	ErrorCode         string          `json:"error_code"`
}

type OperationError struct {
	OperationID string
	Uncertain   bool
	Record      OperationRecord
	Err         error
	Receipt     json.RawMessage
}

func (e *OperationError) Error() string {
	if e.Uncertain {
		return fmt.Sprintf("operation %s: outcome unknown: %v", e.OperationID, e.Err)
	}
	return fmt.Sprintf("operation %s: %v", e.OperationID, e.Err)
}

func (e *OperationError) Unwrap() error {
	return e.Err
}

func operationError(id string, record OperationRecord, err error) error {
	var serverError *Error
	operationErr := &OperationError{OperationID: id, Record: record, Receipt: record.Receipt, Uncertain: !errors.As(err, &serverError), Err: err}
	var transportErr *transportFailure
	if errors.As(err, &transportErr) {
		operationErr.Uncertain = true
	}
	if serverError != nil {
		var data struct {
			Receipt json.RawMessage `json:"receipt"`
			Detail  struct {
				Code string `json:"code"`
			} `json:"detail"`
		}
		if json.Unmarshal(serverError.Data, &data) == nil {
			operationErr.Receipt = data.Receipt
			var receipt struct {
				Outcome string `json:"outcome"`
			}
			_ = json.Unmarshal(data.Receipt, &receipt)
			operationErr.Uncertain = operationErr.Uncertain || receipt.Outcome == "outcome_unknown" || data.Detail.Code == "outcome_unknown"
		}
	}
	return operationErr
}

func NewOperationID() (string, error) {
	var id [16]byte
	if _, err := rand.Read(id[:]); err != nil {
		return "", err
	}
	id[6] = (id[6] & 0x0f) | 0x40
	id[8] = (id[8] & 0x3f) | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", id[:4], id[4:6], id[6:8], id[8:10], id[10:]), nil
}

func (p *Page) Perform(ctx context.Context, id string, action map[string]any, options ActionOptions) (OperationRecord, error) {
	var record OperationRecord
	if err := p.b.t.requireCapability("durableActions"); err != nil {
		return record, operationError(id, record, err)
	}
	if err := ctx.Err(); err != nil {
		return record, &OperationError{OperationID: id, Err: err}
	}
	err := p.call(ctx, "actions.perform", map[string]any{"operation_id": id, "action": action, "options": options}, &record)
	if err != nil {
		return record, operationError(id, record, err)
	}
	return record, nil
}

func (b *Browser) Operation(ctx context.Context, id string) (OperationRecord, error) {
	var record OperationRecord
	if err := b.t.requireCapability("durableActions"); err != nil {
		return record, err
	}
	err := b.t.call(ctx, "operations.get", map[string]any{"id": id}, &record)
	return record, err
}
func (b *Browser) WaitOperation(ctx context.Context, id string, timeoutMS uint64) (OperationRecord, error) {
	var record OperationRecord
	if err := b.t.requireCapability("durableActions"); err != nil {
		return record, err
	}
	err := b.t.call(ctx, "operations.wait", map[string]any{"id": id, "timeout_ms": timeoutMS}, &record)
	return record, err
}
func (b *Browser) CancelOperation(ctx context.Context, id string) (OperationRecord, error) {
	var record OperationRecord
	if err := b.t.requireCapability("operationCancellation"); err != nil {
		return record, err
	}
	err := b.t.call(ctx, "operations.cancel", map[string]any{"id": id}, &record)
	return record, err
}

func (p *Page) performLegacy(ctx context.Context, method string, params map[string]any, options []ActionOptions) error {
	id, err := NewOperationID()
	if err != nil {
		return err
	}
	action := map[string]any{}
	selector := func() map[string]any {
		return map[string]any{"query": map[string]any{"kind": "css", "selector": params["selector"]}}
	}
	switch method {
	case "page.goto":
		action = map[string]any{"kind": "navigate", "url": params["url"]}
	case "page.click", "page.human_click":
		action = map[string]any{"kind": "click", "target": selector(), "human": method == "page.human_click"}
	case "page.click_text", "page.human_click_text":
		action = map[string]any{"kind": "click", "target": map[string]any{"query": map[string]any{"kind": "text", "text": params["text"]}}, "human": method == "page.human_click_text"}
	case "page.fill", "page.human_fill":
		action = map[string]any{"kind": "fill", "target": selector(), "value": params["value"], "human": method == "page.human_fill"}
	case "page.type_into":
		action = map[string]any{"kind": "type", "target": selector(), "text": params["text"]}
	case "page.select":
		action = map[string]any{"kind": "select_option", "target": selector(), "value": params["value"]}
	case "page.hover":
		action = map[string]any{"kind": "hover", "target": selector()}
	case "page.press_key":
		action = map[string]any{"kind": "press_key", "key": params["key"]}
	default:
		return fmt.Errorf("unsupported action %s", method)
	}
	option := ActionOptions{}
	if len(options) > 0 {
		option = options[0]
	}
	record, err := p.Perform(ctx, id, action, option)
	if err != nil {
		return err
	}
	if record.ID != id {
		return &OperationError{OperationID: id, Record: record, Receipt: record.Receipt, Uncertain: true, Err: errors.New("server returned a different operation ID; recover explicitly")}
	}
	for record.State == "running" {
		next, waitErr := p.b.WaitOperation(ctx, id, 30000)
		if waitErr != nil {
			return &OperationError{OperationID: id, Record: record, Receipt: record.Receipt, Uncertain: true, Err: waitErr}
		}
		record = next
	}
	if record.Outcome != "verified_complete" {
		return &OperationError{OperationID: id, Record: record, Receipt: record.Receipt, Uncertain: record.Outcome == "outcome_unknown", Err: fmt.Errorf("%s (%s)", record.Outcome, record.ErrorCode)}
	}
	return nil
}
