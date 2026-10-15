package eoka

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
)

type ProtocolVersion struct {
	Major uint32 `json:"major"`
	Minor uint32 `json:"minor"`
}

type TransportLimits struct {
	MaxMessageBytes int `json:"maxMessageBytes"`
	MaxInFlight     int `json:"maxInFlight"`
}

type ServerInfo struct {
	Protocol      ProtocolVersion `json:"protocol"`
	ServerVersion string          `json:"serverVersion"`
	CoreVersion   string          `json:"coreVersion"`
	Methods       []string        `json:"methods"`
	Capabilities  []string        `json:"capabilities"`
	Limits        TransportLimits `json:"limits"`
}

func (b *Browser) ServerInfo() ServerInfo {
	info := b.info
	info.Methods = append([]string(nil), info.Methods...)
	info.Capabilities = append([]string(nil), info.Capabilities...)
	return info
}

func (b *Browser) hello(ctx context.Context, requiredMethods ...string) error {
	var raw json.RawMessage
	if err := b.t.call(ctx, "server.hello", map[string]any{"protocol": ProtocolVersion{Major: 1, Minor: 1}}, &raw); err != nil {
		if HasCode(err, ErrCodeUnknownMethod) || HasCode(err, ErrCodeProtocolMismatch) {
			var serverError *Error
			errors.As(err, &serverError)
			return &Error{Code: ErrCodeProtocolMismatch, Data: serverError.Data, Message: fmt.Sprintf("server does not support sidecar protocol 1.1 (%v); provide a compatible eoka-server with WithServerPath or EOKA_SERVER_BIN; the pinned 0.3.0 download is not upgraded automatically", err)}
		}
		return fmt.Errorf("eoka: server.hello failed before browser session creation: %w", err)
	}
	var shape struct {
		Protocol *struct {
			Major *uint32 `json:"major"`
			Minor *uint32 `json:"minor"`
		} `json:"protocol"`
	}
	var info ServerInfo
	if err := json.Unmarshal(raw, &info); err != nil {
		return &Error{Code: ErrCodeProtocolMismatch, Message: "invalid server.hello result: " + err.Error()}
	}
	if err := json.Unmarshal(raw, &shape); err != nil || shape.Protocol == nil || shape.Protocol.Major == nil || shape.Protocol.Minor == nil {
		return &Error{Code: ErrCodeProtocolMismatch, Message: "server.hello omitted its protocol version"}
	}
	if info.Protocol.Major != 1 || info.Protocol.Minor < 1 {
		return &Error{Code: ErrCodeProtocolMismatch, Message: fmt.Sprintf("server protocol %d.%d is incompatible with client protocol 1.1", info.Protocol.Major, info.Protocol.Minor)}
	}
	if info.ServerVersion == "" || info.CoreVersion == "" || info.Limits.MaxMessageBytes <= 0 || info.Limits.MaxInFlight <= 0 || info.Methods == nil || info.Capabilities == nil {
		return &Error{Code: ErrCodeProtocolMismatch, Message: "server.hello omitted versions, methods, capabilities or positive transport limits"}
	}
	methods := make(map[string]bool, len(info.Methods))
	for _, method := range info.Methods {
		methods[method] = true
	}
	capabilities := make(map[string]bool, len(info.Capabilities))
	for _, capability := range info.Capabilities {
		capabilities[capability] = true
	}
	if len(requiredMethods) == 0 {
		requiredMethods = []string{"browser.launch"}
	}
	for _, method := range append([]string{"server.hello", "server.shutdown"}, requiredMethods...) {
		if !methods[method] {
			return &Error{Code: ErrCodeProtocolMismatch, Message: "server does not advertise required method " + method}
		}
	}
	if !capabilities["gracefulShutdown"] {
		return &Error{Code: ErrCodeProtocolMismatch, Message: "server does not advertise required capability gracefulShutdown"}
	}
	b.t.mu.Lock()
	b.t.methods = methods
	b.t.capabilities = capabilities
	b.t.messageLimit = min(maxMessageBytes, info.Limits.MaxMessageBytes)
	b.t.inFlightLimit = min(maxInFlight, info.Limits.MaxInFlight)
	b.t.mu.Unlock()
	b.info = info
	return nil
}
