import { chmodSync, copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const appDirectory = resolve(scriptDirectory, "..");
const source = resolve(appDirectory, "target/release/open-islandd");
const destinationDirectory = resolve(appDirectory, "src-tauri/binaries");
const destination = resolve(
  destinationDirectory,
  "open-islandd-x86_64-unknown-linux-gnu",
);

if (!existsSync(source)) {
  console.error(
    `stage-sidecar: missing daemon binary at ${source}. Run \`cargo build --release -p open-islandd\` from ${appDirectory} first.`,
  );
  process.exit(1);
}

mkdirSync(destinationDirectory, { recursive: true });
copyFileSync(source, destination);
chmodSync(destination, 0o755);
console.log(`stage-sidecar: ${source} -> ${destination}`);
