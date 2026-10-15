export async function resolve(specifier, context, nextResolve) {
  if (specifier === './browser-install.js' && context.parentURL?.endsWith('/browser.js')) {
    const engineUrl = new URL('./browser-install-engine.js', context.parentURL).href;
    const source = `
      import { appendFileSync } from 'node:fs';
      import { createBrowserInstaller } from ${JSON.stringify(engineUrl)};
      const release = JSON.parse(process.env.EOKA_LIFECYCLE_INSTALL_RELEASE);
      export async function installBrowser(options) {
        appendFileSync(process.env.EOKA_BROWSER_FIXTURE_LOG, JSON.stringify({event:'installer-called', download:options.download, timeoutMs:options.timeoutMs})+'\\n');
        return await createBrowserInstaller(release)(options);
      }
    `;
    return { url: `data:text/javascript,${encodeURIComponent(source)}`, shortCircuit: true };
  }
  return nextResolve(specifier, context);
}
