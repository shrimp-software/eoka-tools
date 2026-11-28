// Generated from the eoka schema bundle by scripts/generate-protocol.mjs. Do not edit.
import type { MethodName } from "./protocol.js";

export const serverWaits: { readonly [M in MethodName]?: Readonly<Record<string, number>> } = {
  "actions.perform": {
    "options.timeout_ms": 30000
  },
  "agent.observe": {
    "timeout_ms": 3000
  },
  "agent.step": {
    "action_timeout_ms": 30000,
    "timeout_ms": 10000
  },
  "back": {
    "timeout_ms": 0
  },
  "browser.connect": {
    "autoSolveCloudflare.timeoutMs": 30000
  },
  "browser.launch": {
    "autoSolveCloudflare.timeoutMs": 30000
  },
  "captcha.datadome": {
    "timeout_ms": 60000
  },
  "captcha.turnstile": {
    "timeout_ms": 30000
  },
  "click": {
    "timeout_ms": 0
  },
  "click_shadow": {
    "timeout_ms": 10000
  },
  "double_click": {
    "timeout_ms": 0
  },
  "download.start": {
    "start_timeout_ms": 30000
  },
  "download.wait": {
    "timeout_ms": 30000
  },
  "fill": {
    "timeout_ms": 0
  },
  "forward": {
    "timeout_ms": 0
  },
  "hover": {
    "timeout_ms": 0
  },
  "key": {
    "timeout_ms": 0
  },
  "network.export": {
    "settle_ms": 0
  },
  "network.wait": {
    "timeout": 0
  },
  "open": {
    "timeout_ms": 0
  },
  "operations.wait": {
    "timeout_ms": 30000
  },
  "scroll": {
    "timeout_ms": 0
  },
  "select": {
    "timeout_ms": 0
  },
  "wait": {
    "ms": 0,
    "timeout": 0
  }
};
