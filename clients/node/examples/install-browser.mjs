import { installBrowser } from "@cbxss/eoka";

const controller = new AbortController();
const abort = () => controller.abort();
process.once("SIGINT", abort);
process.once("SIGTERM", abort);
try {
  const installation = await installBrowser({
    ...(process.argv[2] === undefined ? {} : { browserCacheDir: process.argv[2] }),
    signal: controller.signal,
    timeoutMs: 300000,
    onProgress: ({ downloadedBytes, totalBytes, buildId }) => {
      process.stderr.write(`Chrome ${buildId}: ${downloadedBytes}/${totalBytes} bytes\n`);
    },
  });
  console.log(JSON.stringify(installation, null, 2));
} finally {
  process.removeListener("SIGINT", abort);
  process.removeListener("SIGTERM", abort);
}
