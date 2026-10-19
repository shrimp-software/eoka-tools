package eoka

import (
	"context"
	"encoding/base64"
	"encoding/json"
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

func (p *Page) ID() string { return p.id }

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
	return p.call(ctx, "page.set_viewport", map[string]any{
		"width": width, "height": height, "deviceScaleFactor": scale,
	}, nil)
}

func (p *Page) ResetViewport(ctx context.Context) error {
	return p.call(ctx, "page.reset_viewport", nil, nil)
}

func (p *Page) Goto(ctx context.Context, url string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.goto", map[string]any{"url": url}, options)
}

func (p *Page) Click(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.click", map[string]any{"selector": selector}, options)
}

func (p *Page) ClickText(ctx context.Context, text string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.click_text", map[string]any{"text": text}, options)
}

func (p *Page) HumanClick(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.human_click", map[string]any{"selector": selector}, options)
}

func (p *Page) HumanClickText(ctx context.Context, text string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.human_click_text", map[string]any{"text": text}, options)
}

func (p *Page) Fill(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.fill", map[string]any{"selector": selector, "value": value}, options)
}

func (p *Page) HumanFill(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.human_fill", map[string]any{"selector": selector, "value": value}, options)
}

func (p *Page) TypeInto(ctx context.Context, selector, text string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.type_into", map[string]any{"selector": selector, "text": text}, options)
}

func (p *Page) Text(ctx context.Context) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.text", nil, &res)
	return res.Text, err
}

func (p *Page) Content(ctx context.Context) (string, error) {
	var res struct {
		HTML string `json:"html"`
	}
	err := p.call(ctx, "page.content", nil, &res)
	return res.HTML, err
}

func (p *Page) Title(ctx context.Context) (string, error) {
	var res struct {
		Title string `json:"title"`
	}
	err := p.call(ctx, "page.title", nil, &res)
	return res.Title, err
}

func (p *Page) URL(ctx context.Context) (string, error) {
	var res struct {
		URL string `json:"url"`
	}
	err := p.call(ctx, "page.url", nil, &res)
	return res.URL, err
}

func (p *Page) GetText(ctx context.Context, selector string) (string, error) {
	var res struct {
		Text string `json:"text"`
	}
	err := p.call(ctx, "page.get_text", map[string]any{"selector": selector}, &res)
	return res.Text, err
}

func (p *Page) GetAttribute(ctx context.Context, selector, name string) (value string, ok bool, err error) {
	var res struct {
		Value *string `json:"value"`
	}
	if err = p.call(ctx, "page.get_attribute", map[string]any{"selector": selector, "name": name}, &res); err != nil {
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
	err := p.call(ctx, "page.exists", map[string]any{"selector": selector}, &res)
	return res.Exists, err
}

func (p *Page) WaitFor(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForVisible(ctx context.Context, selector string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_visible", map[string]any{"selector": selector, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) WaitForText(ctx context.Context, text string, timeout time.Duration) error {
	return p.call(ctx, "page.wait_for_text", map[string]any{"text": text, "timeoutMs": timeout.Milliseconds()}, nil)
}

func (p *Page) Evaluate(ctx context.Context, js string) (json.RawMessage, error) {
	var res struct {
		Result json.RawMessage `json:"result"`
	}
	err := p.call(ctx, "page.evaluate", map[string]any{"js": js}, &res)
	return res.Result, err
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

func (p *Page) Execute(ctx context.Context, js string) error {
	return p.call(ctx, "page.execute", map[string]any{"js": js}, nil)
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
	var response FetchResponse
	err := p.call(ctx, "page.fetch", params, &response)
	return response, err
}

// TurnstileResult reports how a Cloudflare Turnstile challenge was cleared.
type TurnstileResult struct {
	Clicks    int   `json:"clicks"`
	ElapsedMs int64 `json:"elapsedMs"`
}

// SolveTurnstile clicks the Cloudflare Turnstile checkbox until the page
// leaves the challenge. It returns zero clicks on a normal page. A zero
// timeout uses the server default of 30s.
func (p *Page) SolveTurnstile(ctx context.Context, timeout time.Duration) (TurnstileResult, error) {
	params := map[string]any{}
	if timeout != 0 {
		if err := validateTurnstileTimeout(timeout); err != nil {
			return TurnstileResult{}, err
		}
		params["timeoutMs"] = timeout.Milliseconds()
	}
	var result TurnstileResult
	err := p.call(ctx, "page.solve_turnstile", params, &result)
	return result, err
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
	if err := p.call(ctx, "page.screenshot", nil, &res); err != nil {
		return nil, err
	}
	data, err := base64.StdEncoding.DecodeString(res.DataBase64)
	if err != nil {
		return nil, fmt.Errorf("eoka: decoding screenshot data: %w", err)
	}
	return data, nil
}

func (p *Page) Select(ctx context.Context, selector, value string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.select", map[string]any{"selector": selector, "value": value}, options)
}

func (p *Page) Hover(ctx context.Context, selector string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.hover", map[string]any{"selector": selector}, options)
}

func (p *Page) PressKey(ctx context.Context, key string, options ...ActionOptions) error {
	return p.performLegacy(ctx, "page.press_key", map[string]any{"key": key}, options)
}

func (p *Page) MouseDown(ctx context.Context, x, y float64, button MouseButton) error {
	return p.call(ctx, "page.mouse_down", map[string]any{"x": x, "y": y, "button": button}, nil)
}

func (p *Page) MouseMove(ctx context.Context, x, y float64) error {
	return p.call(ctx, "page.mouse_move", map[string]any{"x": x, "y": y}, nil)
}

func (p *Page) MouseUp(ctx context.Context, x, y float64, button MouseButton) error {
	return p.call(ctx, "page.mouse_up", map[string]any{"x": x, "y": y, "button": button}, nil)
}

func (p *Page) KeyDown(ctx context.Context, key string) error {
	return p.call(ctx, "page.key_down", map[string]any{"key": key}, nil)
}

func (p *Page) KeyUp(ctx context.Context, key string) error {
	return p.call(ctx, "page.key_up", map[string]any{"key": key}, nil)
}

func (p *Page) ReleaseAllInputs(ctx context.Context) error {
	return p.call(ctx, "page.release_all_inputs", nil, nil)
}

func (p *Page) Close(ctx context.Context) error {
	return p.call(ctx, "page.close", nil, nil)
}
