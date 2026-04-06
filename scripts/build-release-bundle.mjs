import { cpSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(".");
const dist = resolve("dist");
const releaseDir = resolve("release");
const platform = process.platform;
const arch = process.arch === "x64" ? "x86_64" : process.arch;
const bundleName = `memovyn-${platform}-${arch}`;
const outputDir = join(releaseDir, bundleName);

if (!existsSync(dist)) {
  throw new Error("Build output not found. Run `npm run build` first.");
}

rmSync(outputDir, { recursive: true, force: true });
mkdirSync(outputDir, { recursive: true });

for (const entry of [
  "dist",
  "static",
  "examples",
  "package.json",
  "README.md",
  "CHANGELOG.md",
  "CONTRIBUTING.md",
  "LICENSE"
]) {
  cpSync(join(root, entry), join(outputDir, entry), { recursive: true });
}

if (existsSync(join(root, "node_modules"))) {
  cpSync(join(root, "node_modules"), join(outputDir, "node_modules"), { recursive: true });
}

writeFileSync(
  join(outputDir, platform === "win32" ? "memovyn.cmd" : "memovyn"),
  platform === "win32"
    ? "@echo off\r\nnode dist\\cli.mjs %*\r\n"
    : "#!/usr/bin/env sh\nnode dist/cli.mjs \"$@\"\n",
  "utf8"
);
