package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"time"
)

type Page struct {
	id string
	b  *Browser
}

type MouseButton string

const (
	MouseButtonLeft    MouseButton = "left"
	MouseButtonMiddle  MouseButton = "middle"
	MouseButtonRight   MouseButton = "right"
	MouseButtonBack    MouseButton = "back"
	MouseButtonForward MouseButton = "forward"
)

type CaptchaOptions struct {
	APIKey            string          `json:"-"`
	Mode              string          `json:"-"`
	Type              string          `json:"-"`
	WebsiteURL        string          `json:"-"`
	WebsiteKey        string          `json:"-"`
	EnterprisePayload json.RawMessage `json:"-"`
	APIDomain         string          `json:"-"`
	Callback          string          `json:"-"`
}

type CaptchaInjection struct {
	Kind            string   `json:"kind"`
	UpdatedCount    int      `json:"updated_count"`
	Created         []string `json:"created"`
	Callbacks       []string `json:"callbacks"`
	Errors          []string `json:"errors"`
	SolverUserAgent string   `json:"solverUserAgent"`
}

type FetchOptions struct {
	Method   string
	Headers  map[string]string
	Body     string
	Redirect string
}

type FetchResponse struct {
	URL     string            `json:"url"`
	Status  int               `json:"status"`
	OK      bool              `json:"ok"`
	Headers map[string]string `json:"headers"`
	Body    string            `json:"body"`
}

const maxFetchBytes = 16 * 1024 * 1024

func (p *Page) ID() string { return p.id }

func (p *Page) Invoke(ctx context.Context, method string, params map[string]any, result any) error {
	return p.call(ctx, method, params, result)
}

func (p *Page) call(ctx context.Context, method string, params map[string]any, result any) error {
	if params == nil {
		params = map[string]any{}
	}
	params["pageId"] = p.id
	return p.b.t.call(ctx, method, params, result)
}

func (p *Page) SetViewport(ctx context.Context, width, height int, scale float64) error {
	if err := validateViewport(width, height); err != nil {
		return err
	}
	if err := validateScale(scale); err != nil {
		return err
	}
	return p.call(ctx, "emulate", map[string]any{
		"width": width, "height": height, "dpr": scale, "desktop": true,
	}, nil)
}

func (p *Page) ResetViewport(ctx context.Context) error {
	return p.call(ctx, "emulate", map[string]any{"reset": true}, nil)
}

func (p *Page) Goto(ctx context.Context, url string, options ...ActionOptions) error {
	return p.performAndWait(ctx, map[string]any{"kind": "navigate", "url": url}, options)
}

func (p *Page) Click(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performAndWait(ctx, clickAction(cssTarget(selector), false), options)
}

func (p *Page) ClickText(ctx context.Context, text string, options ...ActionOptions) error {
	return p.performAndWait(ctx, clickAction(textTarget(text), false), options)
}

func (p *Page) HumanClick(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performAndWait(ctx, clickAction(cssTarget(selector), true), options)
}

func (p *Page) HumanClickText(ctx context.Context, text string, options ...ActionOptions) error {
	return p.performAndWait(ctx, clickAction(textTarget(text), true), options)
}

func (p *Page) Fill(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performAndWait(ctx, fillAction(selector, value, ""), options)
}

func (p *Page) HumanFill(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performAndWait(ctx, fillAction(selector, value, "sequential"), options)
}

func (p *Page) TypeInto(ctx context.Context, selector, text string, options ...ActionOptions) error {
	return p.performAndWait(ctx, typeAction(selector, text, ""), options)
}

func (p *Page) Text(ctx context.Context) (string, error) {
	var text string
	err := p.call(ctx, "text", nil, &text)
	return text, err
}

func (p *Page) Content(ctx context.Context) (string, error) {
	var html string
	err := p.call(ctx, "content", nil, &html)
	return html, err
}

func (p *Page) Title(ctx context.Context) (string, error) {
	info, err := p.info(ctx)
	return info.Title, err
}

func (p *Page) URL(ctx context.Context) (string, error) {
	info, err := p.info(ctx)
	return info.URL, err
}

type pageInfo struct {
	URL   string `json:"url"`
	Title string `json:"title"`
}

func (p *Page) info(ctx context.Context) (pageInfo, error) {
	var info pageInfo
	err := p.call(ctx, "info", nil, &info)
	return info, err
}

func (p *Page) GetText(ctx context.Context, selector string) (string, error) {
	var text string
	err := p.call(ctx, "get_text", map[string]any{"selector": selector}, &text)
	return text, err
}

func (p *Page) GetAttribute(ctx context.Context, selector, name string) (value string, ok bool, err error) {
	var res struct {
		Value *string `json:"value"`
	}
	if err = p.call(ctx, "get_attribute", map[string]any{"selector": selector, "name": name}, &res); err != nil {
		return "", false, err
	}
	if res.Value == nil {
		return "", false, nil
	}
	return *res.Value, true, nil
}

func (p *Page) Exists(ctx context.Context, selector string) (bool, error) {
	var res struct {
		Exists bool `json:"exists"`
	}
	err := p.call(ctx, "exists", map[string]any{"selector": selector}, &res)
	return res.Exists, err
}

func (p *Page) WaitFor(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "wait", map[string]any{"selector": selector, "state": "attached", "timeout": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForVisible(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "wait", map[string]any{"selector": selector, "state": "visible", "timeout": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForText(ctx context.Context, text string, timeout time.Duration) error {
	return p.call(ctx, "wait", map[string]any{"text": text, "timeout": timeout.Milliseconds()}, nil)
}

func (p *Page) Evaluate(ctx context.Context, function string, arguments ...any) (json.RawMessage, error) {
	var result json.RawMessage
	err := p.call(ctx, "eval", map[string]any{"code": function, "function": true, "arguments": append([]any{}, arguments...)}, &result)
	return result, err
}

func EvaluateAs[T any](ctx context.Context, page *Page, js string) (T, error) {
	var out T
	raw, err := page.Evaluate(ctx, js)
	if err != nil {
		return out, err
	}
	if len(raw) == 0 || string(raw) == "null" {
		return out, nil
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return out, fmt.Errorf("eoka: decoding evaluate result: %w", err)
	}
	return out, nil
}

func (p *Page) Execute(ctx context.Context, function string, arguments ...any) error {
	return p.call(ctx, "exec", map[string]any{"code": function, "function": true, "arguments": append([]any{}, arguments...)}, nil)
}

func (p *Page) Fetch(ctx context.Context, url string, options FetchOptions) (FetchResponse, error) {
	params := map[string]any{"url": url}
	if options.Method != "" {
		params["method"] = options.Method
	}
	if len(options.Headers) != 0 {
		params["headers"] = options.Headers
	}
	if options.Body != "" {
		params["body"] = options.Body
	}
	if options.Redirect != "" {
		params["redirect"] = options.Redirect
	}
	params["max_body"] = maxFetchBytes
	var response struct {
		URL         string            `json:"url"`
		Status      int               `json:"status"`
		Headers     map[string]string `json:"headers"`
		Body        string            `json:"body"`
		Truncated   bool              `json:"truncated"`
		BodyOmitted *string           `json:"bodyOmitted"`
	}
	if err := p.call(ctx, "fetch", params, &response); err != nil {
		return FetchResponse{}, err
	}
	if response.Truncated {
		return FetchResponse{}, fmt.Errorf("eoka: fetch response body exceeds %d bytes", maxFetchBytes)
	}
	if response.BodyOmitted != nil && *response.BodyOmitted == "binary" {
		return FetchResponse{}, errors.New("eoka: fetch response body is binary and was omitted")
	}
	return FetchResponse{
		URL:     response.URL,
		Status:  response.Status,
		OK:      response.Status >= 200 && response.Status < 300,
		Headers: response.Headers,
		Body:    response.Body,
	}, nil
}

type TurnstileResult struct {
	Clicks    int   `json:"clicks"`
	ElapsedMs int64 `json:"elapsedMs"`
}

func (p *Page) SolveTurnstile(ctx context.Context, timeout time.Duration) (TurnstileResult, error) {
	params := map[string]any{}
	if timeout != 0 {
		if err := validateTurnstileTimeout(timeout); err != nil {
			return TurnstileResult{}, err
		}
		params["timeout_ms"] = timeout.Milliseconds()
	}
	var solved struct {
		Clicks    int   `json:"clicks"`
		ElapsedMs int64 `json:"elapsed_ms"`
	}
	err := p.call(ctx, "captcha.turnstile", params, &solved)
	return TurnstileResult{Clicks: solved.Clicks, ElapsedMs: solved.ElapsedMs}, err
}

func (p *Page) SolveCaptcha(ctx context.Context, options CaptchaOptions) (CaptchaInjection, error) {
	params := map[string]any{
		"apiKey":      options.APIKey,
		"captchaMode": options.Mode,
		"captchaType": options.Type,
		"websiteURL":  options.WebsiteURL,
		"websiteKey":  options.WebsiteKey,
	}
	if len(options.EnterprisePayload) != 0 {
		var payload any
		if err := json.Unmarshal(options.EnterprisePayload, &payload); err != nil {
			return CaptchaInjection{}, fmt.Errorf("eoka: decoding enterprise payload: %w", err)
		}
		params["enterprisePayload"] = payload
	}
	if options.APIDomain != "" {
		params["apiDomain"] = options.APIDomain
	}
	if options.Callback != "" {
		params["callback"] = options.Callback
	}
	var result struct {
		Injection CaptchaInjection `json:"injection"`
	}
	err := p.call(ctx, "page.solve_captcha", params, &result)
	return result.Injection, err
}

func (p *Page) CaptureState(ctx context.Context) (BrowserState, error) {
	var res struct {
		State BrowserState `json:"state"`
	}
	err := p.call(ctx, "page.capture_state", nil, &res)
	return res.State, err
}

func (p *Page) RestoreState(ctx context.Context, state BrowserState) error {
	return p.call(ctx, "page.restore_state", map[string]any{"state": state}, nil)
}

func (p *Page) Screenshot(ctx context.Context) ([]byte, error) {
	var res struct {
		DataBase64 string `json:"dataBase64"`
	}
	if err := p.call(ctx, "screenshot", map[string]any{"inline": true}, &res); err != nil {
		return nil, err
	}
	data, err := base64.StdEncoding.DecodeString(res.DataBase64)
	if err != nil {
		return nil, fmt.Errorf("eoka: decoding screenshot data: %w", err)
	}
	return data, nil
}

func (p *Page) Select(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performAndWait(ctx, map[string]any{"kind": "select_option", "target": cssTarget(selector), "value": value}, options)
}

func (p *Page) Hover(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performAndWait(ctx, map[string]any{"kind": "hover", "target": cssTarget(selector)}, options)
}

func (p *Page) PressKey(ctx context.Context, key string, options ...ActionOptions) error {
	return p.performAndWait(ctx, map[string]any{"kind": "press_key", "key": key}, options)
}

func (p *Page) MouseDown(ctx context.Context, x, y float64, button MouseButton) error {
	return p.call(ctx, "mouse.down", map[string]any{"x": x, "y": y, "button": button}, nil)
}

func (p *Page) MouseMove(ctx context.Context, x, y float64) error {
	return p.call(ctx, "mouse.move", map[string]any{"x": x, "y": y}, nil)
}

func (p *Page) MouseUp(ctx context.Context, x, y float64, button MouseButton) error {
	return p.call(ctx, "mouse.up", map[string]any{"x": x, "y": y, "button": button}, nil)
}

func (p *Page) KeyDown(ctx context.Context, key string) error {
	return p.call(ctx, "key.down", map[string]any{"key": key}, nil)
}

func (p *Page) KeyUp(ctx context.Context, key string) error {
	return p.call(ctx, "key.up", map[string]any{"key": key}, nil)
}

func (p *Page) ReleaseAllInputs(ctx context.Context) error {
	return p.call(ctx, "release_all_inputs", nil, nil)
}

func (p *Page) Close(ctx context.Context) error {
	var closed struct {
		CleanupError *string `json:"cleanupError"`
	}
	if err := p.call(ctx, "browser.close_tab", nil, &closed); err != nil {
		return err
	}
	if closed.CleanupError != nil {
		return fmt.Errorf("eoka: tab closed but cleanup failed: %s", *closed.CleanupError)
	}
	return nil
}

func (p *Page) EvaluateRaw(ctx context.Context, js string) (json.RawMessage, error) {
	var result json.RawMessage
	err := p.call(ctx, "eval", map[string]any{"code": js, "raw": true, "json": true}, &result)
	return result, err
}

func (p *Page) ExecuteRaw(ctx context.Context, js string) error {
	return p.call(ctx, "exec", map[string]any{"code": js, "raw": true}, nil)
}

func (p *Page) FillWithMode(ctx context.Context, selector, value, inputMode string, options ...ActionOptions) error {
	return p.performAndWait(ctx, fillAction(selector, value, inputMode), options)
}

func (p *Page) TypeWithMode(ctx context.Context, selector, text, inputMode string, options ...ActionOptions) error {
	return p.performAndWait(ctx, typeAction(selector, text, inputMode), options)
}

func cssTarget(selector string) map[string]any {
	return map[string]any{"query": map[string]any{"kind": "css", "selector": selector}}
}

func textTarget(text string) map[string]any {
	return map[string]any{"query": map[string]any{"kind": "text", "text": text}}
}

func clickAction(target map[string]any, human bool) map[string]any {
	return map[string]any{"kind": "click", "target": target, "human": human}
}

func fillAction(selector, value, inputMode string) map[string]any {
	action := map[string]any{"kind": "fill", "target": cssTarget(selector), "value": value}
	if inputMode != "" {
		action["input_mode"] = inputMode
	}
	return action
}

func typeAction(selector, text, inputMode string) map[string]any {
	action := map[string]any{"kind": "type", "target": cssTarget(selector), "text": text}
	if inputMode != "" {
		action["input_mode"] = inputMode
	}
	return action
}
