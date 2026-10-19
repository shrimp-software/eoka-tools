package eoka

import (
	"context"
	"errors"
	"fmt"
	"io"
	"math"
	"net"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const closeTimeout = 45 * time.Second

var ErrServerBinaryNotFound = errors.New("eoka: eoka-server binary not found (set EOKA_SERVER_BIN, pass eoka.WithServerPath, or add eoka-server to PATH)")

type Browser struct {
	cmd         *exec.Cmd
	t           *transport
	info        ServerInfo
	session     BrowserSessionInfo
	processDone chan struct{}
	processErr  error
	closeOnce   sync.Once
	closeDone   chan struct{}
	closeErr    error
}

type TabInfo struct {
	ID    string `json:"id"`
	Title string `json:"title"`
	URL   string `json:"url"`
}

type options struct {
	headless             bool
	userAgent            string
	timezone             string
	viewportWidth        int
	viewportHeight       int
	proxy                string
	userDataDir          string
	serverPath           string
	stderr               io.Writer
	noAutoDownload       bool
	executablePath       string
	executablePathSet    bool
	runtimeDir           string
	runtimeDirSet        bool
	sandbox              bool
	deviceScaleFactor    float64
	deviceScaleFactorSet bool
	viewportSet          bool
	launchOnly           bool
	autoSolveCloudflare  bool
	cloudflareInterval   time.Duration
	cloudflareTimeout    time.Duration
}

type BrowserOwnership string

const (
	BrowserOwnershipOwned    BrowserOwnership = "owned"
	BrowserOwnershipBorrowed BrowserOwnership = "borrowed"
)

type BrowserSessionInfo struct {
	Ownership      BrowserOwnership `json:"ownership"`
	BrowserVersion string           `json:"browserVersion"`
}

func (b *Browser) SessionInfo() BrowserSessionInfo {
	return b.session
}

type Option func(*options)

func WithHeadless(headless bool) Option {
	return func(o *options) {
		o.headless = headless
		o.launchOnly = true
	}
}

func WithUserAgent(userAgent string) Option {
	return func(o *options) {
		o.userAgent = userAgent
		o.launchOnly = true
	}
}

func WithTimezone(timezone string) Option {
	return func(o *options) {
		o.timezone = timezone
		o.launchOnly = true
	}
}

func WithViewport(width, height int) Option {
	return func(o *options) {
		o.viewportWidth = width
		o.viewportHeight = height
		o.viewportSet = true
		o.launchOnly = true
	}
}

func WithProxy(proxyURL string) Option {
	return func(o *options) {
		o.proxy = proxyURL
		o.launchOnly = true
	}
}

func WithUserDataDir(path string) Option {
	return func(o *options) {
		o.userDataDir = path
		o.launchOnly = true
	}
}

func WithExecutablePath(path string) Option {
	return func(o *options) {
		o.executablePath = path
		o.executablePathSet = true
		o.launchOnly = true
	}
}

func WithRuntimeDir(path string) Option {
	return func(o *options) {
		o.runtimeDir = path
		o.runtimeDirSet = true
		o.launchOnly = true
	}
}

func WithSandbox(enabled bool) Option {
	return func(o *options) {
		o.sandbox = enabled
		o.launchOnly = true
	}
}

func WithDeviceScaleFactor(scale float64) Option {
	return func(o *options) {
		o.deviceScaleFactor = scale
		o.deviceScaleFactorSet = true
		o.launchOnly = true
	}
}

func WithVisible() Option {
	return WithHeadless(false)
}

// WithAutoSolveCloudflare has the server watch every tab and solve full-page
// Cloudflare challenges. Zero durations use the server defaults (500ms poll,
// 30s per solve). It applies to both Launch and Connect.
func WithAutoSolveCloudflare(interval, timeout time.Duration) Option {
	return func(o *options) {
		o.autoSolveCloudflare = true
		o.cloudflareInterval = interval
		o.cloudflareTimeout = timeout
	}
}

func (o options) cloudflareParams() (map[string]any, error) {
	params := map[string]any{}
	if o.cloudflareInterval != 0 {
		if o.cloudflareInterval < 100*time.Millisecond || o.cloudflareInterval > time.Minute {
			return nil, errors.New("eoka: Cloudflare poll interval must be between 100ms and 60s")
		}
		params["intervalMs"] = o.cloudflareInterval.Milliseconds()
	}
	if o.cloudflareTimeout != 0 {
		if err := validateTurnstileTimeout(o.cloudflareTimeout); err != nil {
			return nil, err
		}
		params["timeoutMs"] = o.cloudflareTimeout.Milliseconds()
	}
	return params, nil
}

func validateTurnstileTimeout(timeout time.Duration) error {
	if timeout < time.Millisecond || timeout > 2*time.Minute {
		return errors.New("eoka: Turnstile timeout must be between 1ms and 120s")
	}
	return nil
}

func WithServerPath(path string) Option {
	return func(o *options) { o.serverPath = path }
}

func WithStderr(w io.Writer) Option {
	return func(o *options) { o.stderr = w }
}

func WithNoAutoDownload() Option {
	return func(o *options) { o.noAutoDownload = true }
}

func resolveServerPath(ctx context.Context, explicit string, allowDownload bool) (string, error) {
	if explicit != "" {
		return explicit, nil
	}
	if p := os.Getenv("EOKA_SERVER_BIN"); p != "" {
		return p, nil
	}
	if p, err := exec.LookPath("eoka-server"); err == nil {
		return p, nil
	}
	if !allowDownload {
		return "", ErrServerBinaryNotFound
	}
	return ensureServerBinary(ctx)
}

func Launch(ctx context.Context, opts ...Option) (*Browser, error) {
	o := options{headless: true, stderr: io.Discard}
	for _, opt := range opts {
		opt(&o)
	}
	launchParams, err := o.launchParams()
	if err != nil {
		return nil, err
	}
	return startBrowser(ctx, o, "browser.launch", launchParams)
}

func Connect(ctx context.Context, wsEndpoint string, opts ...Option) (*Browser, error) {
	o := options{headless: true, stderr: io.Discard}
	for _, opt := range opts {
		opt(&o)
	}
	if o.launchOnly {
		return nil, errors.New("eoka: Connect does not accept browser launch options")
	}
	endpoint, err := url.Parse(wsEndpoint)
	if err != nil || endpoint.Scheme != "ws" || endpoint.Hostname() == "" || endpoint.User != nil || endpoint.RawQuery != "" || endpoint.ForceQuery || endpoint.Fragment != "" || !strings.HasPrefix(endpoint.Path, "/devtools/browser/") || strings.TrimPrefix(endpoint.Path, "/devtools/browser/") == "" || strings.Contains(strings.TrimPrefix(endpoint.Path, "/devtools/browser/"), "/") {
		return nil, errors.New("eoka: Connect requires a ws:// browser DevTools endpoint; wss:// is not supported")
	}
	params := map[string]any{"wsEndpoint": wsEndpoint}
	if o.autoSolveCloudflare {
		cloudflare, err := o.cloudflareParams()
		if err != nil {
			return nil, err
		}
		params["autoSolveCloudflare"] = cloudflare
	}
	return startBrowser(ctx, o, "browser.connect", params)
}

func startBrowser(ctx context.Context, o options, method string, params map[string]any) (*Browser, error) {
	binPath, err := resolveServerPath(ctx, o.serverPath, !o.noAutoDownload)
	if err != nil {
		return nil, err
	}

	cmd := exec.Command(binPath)
	cmd.Stderr = o.stderr
	cmd.WaitDelay = time.Second

	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("eoka: creating stdin pipe: %w", err)
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		_ = stdin.Close()
		return nil, fmt.Errorf("eoka: creating stdout pipe: %w", err)
	}

	if err := cmd.Start(); err != nil {
		_ = stdin.Close()
		_ = stdout.Close()
		return nil, fmt.Errorf("eoka: starting %s: %w", binPath, err)
	}

	t := newTransport(stdin, stdout)
	go t.readLoop()

	b := &Browser{cmd: cmd, t: t, processDone: make(chan struct{}), closeDone: make(chan struct{})}

	if err := b.hello(ctx, method); err != nil {
		_ = b.Close(context.Background())
		return nil, err
	}
	if _, ok := params["autoSolveCloudflare"]; ok {
		if err := t.requireCapability("cloudflareAutoSolve"); err != nil {
			_ = b.Close(context.Background())
			return nil, err
		}
	}
	if err := t.call(ctx, method, params, &b.session); err != nil {
		_ = b.Close(context.Background())
		return nil, err
	}
	if (b.session.Ownership != BrowserOwnershipOwned && b.session.Ownership != BrowserOwnershipBorrowed) || strings.TrimSpace(b.session.BrowserVersion) == "" || (method == "browser.connect" && b.session.Ownership != BrowserOwnershipBorrowed) {
		_ = b.Close(context.Background())
		return nil, &Error{Code: ErrCodeProtocolMismatch, Message: "invalid browser session ownership or browserVersion"}
	}

	return b, nil
}

func (o options) launchParams() (map[string]any, error) {
	params := map[string]any{"headless": o.headless, "sandbox": o.sandbox}
	if o.autoSolveCloudflare {
		cloudflare, err := o.cloudflareParams()
		if err != nil {
			return nil, err
		}
		params["autoSolveCloudflare"] = cloudflare
	}
	if o.executablePathSet || o.executablePath != "" {
		if !filepath.IsAbs(o.executablePath) {
			return nil, errors.New("eoka: executable path must be absolute")
		}
		params["executablePath"] = o.executablePath
	}
	if o.runtimeDirSet || o.runtimeDir != "" {
		if !filepath.IsAbs(o.runtimeDir) {
			return nil, errors.New("eoka: runtime directory must be absolute")
		}
		params["runtimeDir"] = o.runtimeDir
	}
	if o.deviceScaleFactorSet {
		if err := validateScale(o.deviceScaleFactor); err != nil {
			return nil, err
		}
		params["deviceScaleFactor"] = o.deviceScaleFactor
	}
	if o.userAgent != "" {
		params["userAgent"] = o.userAgent
	}
	if o.timezone != "" {
		params["timezone"] = o.timezone
	}
	if o.userDataDir != "" {
		params["userDataDir"] = o.userDataDir
	}
	if o.viewportSet || o.viewportWidth != 0 || o.viewportHeight != 0 {
		if err := validateViewport(o.viewportWidth, o.viewportHeight); err != nil {
			return nil, err
		}
		params["viewportWidth"] = o.viewportWidth
		params["viewportHeight"] = o.viewportHeight
	}
	if o.proxy == "" {
		return params, nil
	}
	proxy, err := parseProxy(o.proxy)
	if err != nil {
		return nil, err
	}
	params["proxy"] = map[string]any{
		"server":   proxy.server,
		"username": proxy.username,
		"password": proxy.password,
	}
	return params, nil
}

func validateViewport(width, height int) error {
	if width <= 0 || height <= 0 || int64(width) > math.MaxInt32 || int64(height) > math.MaxInt32 {
		return errors.New("eoka: viewport dimensions must be positive and at most 2147483647")
	}
	return nil
}

func validateScale(scale float64) error {
	if scale <= 0 || math.IsNaN(scale) || math.IsInf(scale, 0) {
		return errors.New("eoka: device scale factor must be finite and positive")
	}
	return nil
}

type proxyConfig struct {
	server   string
	username string
	password string
}

func parseProxy(value string) (proxyConfig, error) {
	parsed, err := url.Parse(value)
	if err != nil || (parsed.Scheme != "socks5" && parsed.Scheme != "http") {
		return proxyConfig{}, errors.New("eoka: proxy must use socks5:// or http://")
	}
	if parsed.Hostname() == "" || parsed.Port() == "" {
		return proxyConfig{}, errors.New("eoka: proxy must include host and port")
	}
	if (parsed.Path != "" && parsed.Path != "/") || parsed.RawQuery != "" || parsed.Fragment != "" {
		return proxyConfig{}, errors.New("eoka: proxy cannot include a path, query, or fragment")
	}
	username := ""
	password := ""
	if parsed.User != nil {
		username = parsed.User.Username()
		password, _ = parsed.User.Password()
	}
	if (username == "") != (password == "") {
		return proxyConfig{}, errors.New("eoka: proxy credentials require both username and password")
	}
	return proxyConfig{
		server:   parsed.Scheme + "://" + net.JoinHostPort(parsed.Hostname(), parsed.Port()),
		username: username,
		password: password,
	}, nil
}

func (b *Browser) NewPage(ctx context.Context, url string) (*Page, error) {
	var urlParam any
	if url != "" {
		urlParam = url
	}

	var res struct {
		PageID string `json:"pageId"`
	}
	if err := b.t.call(ctx, "browser.new_page", map[string]any{"url": urlParam}, &res); err != nil {
		return nil, err
	}
	return &Page{id: res.PageID, b: b}, nil
}

func (b *Browser) Tabs(ctx context.Context) ([]TabInfo, error) {
	var res struct {
		Tabs []TabInfo `json:"tabs"`
	}
	err := b.t.call(ctx, "browser.tabs", map[string]any{}, &res)
	return res.Tabs, err
}

func (b *Browser) Page(pageID string) *Page {
	return &Page{id: pageID, b: b}
}

func (b *Browser) CloseTab(ctx context.Context, pageID string) error {
	var result struct {
		CleanupError string `json:"cleanupError"`
	}
	if err := b.t.call(ctx, "browser.close_tab", map[string]any{"pageId": pageID}, &result); err != nil {
		return err
	}
	if result.CleanupError != "" {
		return fmt.Errorf("eoka: browser closed tab but input cleanup failed: %s", result.CleanupError)
	}
	return nil
}

func (b *Browser) Close(ctx context.Context) error {
	b.closeOnce.Do(func() {
		if b.closeDone == nil {
			b.closeDone = make(chan struct{})
		}
		go b.close(closeTimeout)
	})
	select {
	case <-b.closeDone:
		return b.closeErr
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (b *Browser) close(grace time.Duration) {
	defer close(b.closeDone)
	ctx, cancel := context.WithTimeout(context.Background(), grace)
	defer cancel()
	b.closeErr = b.t.call(ctx, "server.shutdown", map[string]any{}, nil)
	b.t.shutdown()
	if b.processDone == nil {
		return
	}
	go func() {
		b.processErr = b.cmd.Wait()
		close(b.processDone)
	}()
	select {
	case <-b.processDone:
		if b.closeErr == nil {
			b.closeErr = b.processErr
		}
	case <-ctx.Done():
		b.killProcess()
		<-b.processDone
		if b.closeErr == nil {
			b.closeErr = fmt.Errorf("eoka: graceful shutdown exceeded %s: %w", grace, ctx.Err())
		}
	}
}

func (b *Browser) killProcess() {
	if b.cmd.Process != nil {
		_ = b.cmd.Process.Kill()
	}
}
