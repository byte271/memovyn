import { copyFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const root = resolve(".");
const dist = resolve("dist");
const releaseDir = resolve("release");
const seaConfig = resolve("sea-config.json");
const outputBlob = resolve("sea-prep.blob");
const platform = process.platform;
const executableName = platform === "win32" ? "memovyn.exe" : "memovyn";
const releaseBinary = join(releaseDir, executableName);

if (!existsSync(join(dist, "cli.js"))) {
  throw new Error("Build output not found. Run `npm run build` first.");
}

rmSync(releaseDir, { recursive: true, force: true });
mkdirSync(releaseDir, { recursive: true });

writeFileSync(
  seaConfig,
  JSON.stringify(
    {
      main: "./dist/cli.js",
      output: "./sea-prep.blob",
      disableExperimentalSEAWarning: true
    },
    null,
    2
  )
);

execFileSync(process.execPath, ["--experimental-sea-config", seaConfig], {
  stdio: "inherit",
  cwd: root
});

copyFileSync(process.execPath, releaseBinary);

try {
  execFileSync(
    process.platform === "win32" ? "npx.cmd" : "npx",
    [
      "postject",
      releaseBinary,
      "NODE_SEA_BLOB",
      outputBlob,
      "--sentinel-fuse",
      "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2"
    ],
    { stdio: "inherit", cwd: root }
  );
} catch (error) {
  console.warn("SEA injection skipped or failed. Release binary may require manual packaging.");
  console.warn(error instanceof Error ? error.message : String(error));
}
