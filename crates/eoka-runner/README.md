# eoka-runner

Config-based browser automation. Define actions in YAML, execute deterministically.

Part of the [eoka-tools](https://github.com/shrimp-software/eoka-tools) workspace.

## Quick Start

```yaml
# automation.yaml
name: "Example"
target:
  url: "https://example.com"
actions:
  - wait_for_network_idle:
      timeout_ms: 5000
  - click:
      text: "More information"
      expect: { kind: url, value: "https://www.iana.org/help/example-domains" }
  - screenshot:
      path: "result.png"
success:
  any:
    - url_contains: "iana.org/help/example-domains"
```

```sh
eoka-runner automation.yaml
```

## Features

- **YAML configs** — define automation flows without writing code
- **Parameterized configs** — `${variable}` substitution for reusable flows
- **30 action types** — navigation, clicking, input, scrolling, cookies, JS, conditionals
- **Text targeting** — click by visible text, not just CSS selectors
- **Human-like actions** — optional mouse movement and typing delays
- **Durable actions** — stable operation IDs, explicit completion evidence, no replay of uncertain input
- **Success conditions** — verify URL or text content after completion
- **Failure screenshots** — capture state on error for debugging

## CLI Usage

```sh
# Run config
eoka-runner config.yaml

# Run with parameters
eoka-runner login.yaml -P email=user@example.com -P password=secret

# Validate without running
eoka-runner config.yaml --check

# Verbose output
eoka-runner config.yaml -v      # info level
eoka-runner config.yaml -vv     # debug level

# Override headless mode
eoka-runner config.yaml --headless

# Quiet (errors only)
eoka-runner config.yaml -q
```

## Config Format

```yaml
name: "Automation Name"

# Parameters (optional) - for reusable configs
params:
  email:
    required: true
    description: "User email"
  timeout:
    default: "5000"
    description: "Wait timeout"

browser:
  headless: false
  proxy: "socks5://user:pass@host:port"  # optional
  user_agent: "Custom UA"               # optional
  viewport:                             # optional
    width: 1920
    height: 1080

target:
  url: "https://example.com"

actions:
  # Use ${param_name} for substitution
  - fill:
      selector: "#email"
      value: "${email}"

success:
  any:  # OR conditions
    - url_contains: "/success"
    - text_contains: "Thank you"
  # or use 'all' for AND conditions

on_failure:
  screenshot: "error_{timestamp}.png"
```

Action steps accept `operation_id` and `timeout_ms`. Click, type and key input
require an `expect` condition that is false before the action. Match the examples
to your application; locators are strict. Whole-workflow retries with more than
one attempt are rejected. Use the operation ID to inspect an uncertain result.
See [OPERATIONS.md](../../OPERATIONS.md) for the canonical `perform` request and
recovery commands.

## Action Types

### Navigation
- `goto: { url }` — Navigate to URL
- `back` — Browser back
- `forward` — Browser forward
- `reload` — Refresh page

### Waiting
- `wait: { ms }` — Fixed delay
- `wait_for_network_idle: { idle_ms, timeout_ms }`
- `wait_for: { selector, timeout_ms }` — Wait for element
- `wait_for_visible: { selector, timeout_ms }`
- `wait_for_hidden: { selector, timeout_ms }`
- `wait_for_text: { text, timeout_ms }`
- `wait_for_url: { contains, timeout_ms }`
- `wait_for_email: { ... }` — Wait for IMAP email, extract link/code

### Clicking
- `click: { selector | text, human, expect, operation_id?, timeout_ms? }`
- `try_click: { selector | text, expect }` — Skip only a target unavailable before dispatch
- `try_click_any: { texts, expect }` — Try candidates until one completes or has an uncertain result

### Input
- `fill: { selector | text, value, human }` — Clear and type
- `type: { selector | text, value, expect }` — Append text
- `clear: { selector | text }` — Clear input field
- `select: { selector | text, value }` — Select dropdown option
- `press_key: { key, expect }` — Press key (Enter, Tab, Escape, ArrowDown, etc.)

### Mouse
- `hover: { selector | text }` — Hover over element

### Cookies
- `set_cookie: { name, value, domain?, path? }` — Set a cookie
- `delete_cookie: { name, domain? }` — Delete a cookie

### JavaScript
- `execute: { js }` — Run arbitrary JavaScript

### Scrolling
- `scroll: { direction, amount }`
- `scroll_to: { selector | text }`

### Debug
- `screenshot: { path }`
- `log: { message }`
- `assert_text: { text }`
- `assert_url: { contains }`

### Control Flow
- `if_text_exists: { text, then, else }`
- `if_selector_exists: { selector, then, else }`
- `repeat: { times, actions }`

### Composition
- `include: { path, params? }` — Include another config's actions

## wait_for_email

Waits for an email via IMAP, extracts a link or code, and optionally acts on it.

```yaml
actions:
  - wait_for_email:
      imap:
        host: "imap.gmail.com"
        port: 993
        tls: true
        username: "${imap_user}"
        password: "${imap_pass}"
        mailbox: "INBOX"
      filter:
        from: "no-reply@example.com"
        subject_contains: "Confirm your email"
        unseen_only: true
        since_minutes: 10
        mark_seen: true
      timeout_ms: 120000
      poll_interval_ms: 2000
      extract:
        link:
          allow_domains: ["example.com"]
      action:
        open_link: {}
```

```yaml
actions:
  - wait_for_email:
      imap:
        host: "imap.gmail.com"
        username: "${imap_user}"
        password: "${imap_pass}"
      filter:
        subject_contains: "Your verification code"
        unseen_only: true
      extract:
        code:
          regex: "(\\d{6})"
      action:
        fill:
          selector: "input[name=code]"
```

## Reusable Flows with Include

Create reusable building blocks:

```yaml
# flows/dismiss_cookies.yaml
name: "Dismiss Cookies"
target:
  url: "about:blank"
actions:
  - try_click_any:
      texts: ["Accept", "Accept All", "OK", "Got it"]
      expect: { kind: hidden, target: { query: { kind: css, selector: "#cookie-banner" } } }
```

```yaml
# flows/login.yaml
name: "Login"
params:
  email: { required: true }
  password: { required: true }
target:
  url: "about:blank"
actions:
  - fill: { selector: "#email", value: "${email}" }
  - fill: { selector: "#password", value: "${password}" }
  - click: { text: "Sign In", expect: { kind: url, value: "/dashboard", contains: true } }
```

Compose them in your main config:

```yaml
name: "Checkout Flow"
target:
  url: "https://shop.example.com"
actions:
  - include: { path: "flows/dismiss_cookies.yaml" }
  - include:
      path: "flows/login.yaml"
      params:
        email: "user@example.com"
        password: "secret"
  - click: { text: "Checkout", expect: { kind: url, value: "/checkout", contains: true } }
```

Include paths are relative to the config file's directory.

## Library Usage

```rust
use eoka_runner::{Config, Params, Runner};

// Simple usage
let config = Config::load("automation.yaml")?;
let mut runner = Runner::new(&config.browser).await?;
let result = runner.run(&config).await?;
println!("Success: {}", result.success);
runner.close().await?;

// With parameters
let params = Params::new()
    .set("email", "user@example.com")
    .set("password", "secret");
let config = Config::load_with_params("login.yaml", &params)?;
```

## Examples

See the `configs/` directory in this crate for example YAML configs.
