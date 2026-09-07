// Cortex's dependency-free, single-file erasable-TypeScript build.
import { stripTypeScriptTypes } from "node:module";
import { readFileSync, writeFileSync, renameSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function build(source, destination) {
  const [major, minor] = process.versions.node.split(".").map(Number);
  if (major !== 22 || minor < 13) throw new Error("Node 22.13+ (22 LTS) is required");
  const input = readFileSync(source, "utf8");
  if (Buffer.byteLength(input) > 1024 * 1024) throw new Error("TypeScript source exceeds 1 MiB");
  const result = stripTypeScriptTypes(input, { mode: "strip" });
  // No evaluation, package scripts, dependency installs, tsconfig or type checking.
  const temporary = destination + ".build-" + process.pid;
  try {
    writeFileSync(temporary, result, { flag: "wx" });
    renameSync(temporary, destination);
  } finally {
    rmSync(temporary, { force: true });
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  if (process.argv.length !== 4) throw new Error("Usage: node build.mjs source.ts artifact.mjs");
  build(process.argv[2], process.argv[3]);
}
