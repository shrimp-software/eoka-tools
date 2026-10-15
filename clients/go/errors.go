package eoka

import (
	"encoding/json"
	"errors"
	"fmt"
)

const (
	ErrCodeElementNotFound        = "ElementNotFound"
	ErrCodeElementNotVisible      = "ElementNotVisible"
	ErrCodeTimeout                = "Timeout"
	ErrCodeRetryExhausted         = "RetryExhausted"
	ErrCodeCdp                    = "Cdp"
	ErrCodeInvalidPage            = "InvalidPage"
	ErrCodeInvalidParams          = "InvalidParams"
	ErrCodeUnknownMethod          = "UnknownMethod"
	ErrCodeInternal               = "Internal"
	ErrCodeProtocolMismatch       = "ProtocolMismatch"
	ErrCodeMessageTooLarge        = "MessageTooLarge"
	ErrCodeServerBusy             = "ServerBusy"
	ErrCodeShuttingDown           = "ShuttingDown"
	ErrCodeCancelled              = "Cancelled"
	ErrCodeCapabilityUnavailable  = "CapabilityUnavailable"
	ErrCodeAction                 = "Action"
	ErrCodeOperation              = "Operation"
	ErrCodeEvaluation             = "Evaluation"
	ErrCodeScreenshotTooLarge     = "ScreenshotTooLarge"
	ErrCodeFileOutput             = "FileOutput"
	ErrCodeRecordingUnsupported   = "RecordingUnsupported"
	ErrCodeRecordingNotFound      = "RecordingNotFound"
	ErrCodeRecordingConflict      = "RecordingConflict"
	ErrCodeRecordingAlreadyActive = "RecordingAlreadyActive"
	ErrCodeRecordingInvalidState  = "RecordingInvalidState"
	ErrCodeRecordingFailed        = "RecordingFailed"
)

type Error struct {
	Code    string
	Message string
	Data    json.RawMessage
}

func (e *Error) Error() string {
	return fmt.Sprintf("eoka: %s: %s", e.Code, e.Message)
}

func HasCode(err error, code string) bool {
	var e *Error
	if errors.As(err, &e) {
		return e.Code == code
	}
	return false
}
