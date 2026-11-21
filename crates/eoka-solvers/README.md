# eoka-solvers

Optional CAPTCHA plugins for [eoka](https://github.com/shrimp-software/eoka). Each solver is a cargo feature, on by default:

| Feature | What it adds | Extra dependencies |
|---|---|---|
| `anti-captcha` | `AntiCaptcha`, a client for the anti-captcha.com API: hCaptcha, reCAPTCHA v2/v3/Enterprise and AWS WAF | ureq with rustls |
| `datadome` | `datadome::DataDomeSolver`, a local DataDome slider solver driving an eoka `Page`. See [DATADOME.md](DATADOME.md). | eoka, image |
| `test-fixtures` | `datadome::fixture`, a local challenge server for tests | |

Token injection (`build_captcha_inject_js`) has no dependencies and is always available.

```toml
[dependencies]
eoka-solvers = { version = "0.1", default-features = false, features = ["datadome"] }
```

eoka-server, eoka-cli and eoka-mcp expose the same `anti-captcha` and `datadome` features and forward them here.
