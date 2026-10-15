export function sanitizeInstallationDiagnostic(message: string, env: NodeJS.ProcessEnv): string {
  for (const [key, value] of Object.entries(env)) {
    if (!/proxy/i.test(key) || !value) continue;
    message = message.replaceAll(value, '[redacted proxy]');
    try {
      const proxy = new URL(value);
      if (proxy.username || proxy.password) {
        const username = decodeURIComponent(proxy.username);
        const password = decodeURIComponent(proxy.password);
        const credentials = [proxy.username, proxy.password, username, password, Buffer.from(`${username}:${password}`).toString('base64')];
        for (const credential of credentials) {
          if (credential) message = message.replaceAll(credential, '[redacted]');
        }
      }
    } catch {}
  }
  return message.replace(/(https?:\/\/)[^/\s@]+@/gi, '$1[redacted]@');
}
