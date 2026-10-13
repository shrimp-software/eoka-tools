import { spawn } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { Type } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
    const config = JSON.parse(readFileSync(process.env.EOKA_EVAL_CONFIG!, "utf8"));
    let calls = 0;
    pi.registerTool({
        name: "browser",
        label: "Browser",
        description: config.instructions,
        parameters: config.backend === "eoka"
            ? Type.Object({ args: Type.Array(Type.String()) })
            : Type.Object({ code: Type.String() }),
        async execute(_id, params, signal) {
            if (++calls > 30) throw new Error("Tool call budget exceeded");
            const started = Date.now();
            const args = config.backend === "eoka"
                ? ["--agent", "--session", config.session, "--no-geo-align", ...(params as any).args]
                : [];
            const result = await new Promise<{ code: number | null; stdout: string; stderr: string }>((resolve, reject) => {
                const child = spawn(config.binary, args, { env: { ...process.env, ...config.env }, signal });
                let stdout = "", stderr = "";
                child.stdout.on("data", chunk => { stdout += chunk; });
                child.stderr.on("data", chunk => { stderr += chunk; });
                const timer = setTimeout(() => child.kill("SIGTERM"), 45000);
                child.on("error", error => { clearTimeout(timer); reject(error); });
                child.on("close", code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
                child.stdin.end(config.backend === "browser-use" ? (params as any).code : undefined);
            });
            appendFileSync(config.toolLog, JSON.stringify({ call: calls, input: params, elapsed_ms: Date.now() - started, ...result }) + "\n");
            return { content: [{ type: "text", text: JSON.stringify(result) }], details: { call: calls } };
        },
    });
}
