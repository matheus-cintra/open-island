import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const appDirectory = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (.+)$/m)?.[1];
const target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET || host;
if (!target || !/^(aarch64|x86_64)-(apple-darwin|unknown-linux-gnu)$/.test(target)) {
  throw new Error(`Arquitetura sem suporte: ${target}`);
}
// Build and stage the exact same target as Tauri, including cross builds.
execFileSync("cargo", ["build", "--release", "-p", "open-islandd", "--target", target], {
  cwd: appDirectory, stdio: "inherit",
  env: { ...process.env, ...(target.endsWith("apple-darwin") ? { MACOSX_DEPLOYMENT_TARGET: "12.0" } : {}) },
});
const targetDirectory = process.env.CARGO_TARGET_DIR || "target";
const source = resolve(appDirectory, targetDirectory, target, "release/open-islandd");
const directory = resolve(appDirectory, "src-tauri/binaries");
mkdirSync(directory, { recursive: true });
const destination = resolve(directory, `open-islandd-${target}`);
copyFileSync(source, destination);
chmodSync(destination, 0o755);
console.log(`stage-sidecar: ${target} -> ${destination}`);
