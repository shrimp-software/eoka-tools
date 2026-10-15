import { createBrowserInstaller } from '../../dist/browser-install-engine.js';

const { release, options } = JSON.parse(process.argv[2]);
try {
  const result = await createBrowserInstaller(release)(options);
  process.send?.({ result });
} catch (error) {
  process.send?.({ error: { code: error.code, message: error.message } });
  process.exitCode = 1;
} finally {
  process.disconnect?.();
}
