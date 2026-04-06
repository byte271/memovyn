import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
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
const seaEntry = join(dist, "cli.cjs");
const postjectBinary = resolve(
  "node_modules",
  ".bin",
  platform === "win32" ? "postject.cmd" : "postject"
);

if (!existsSync(seaEntry)) {
  const distEntries = existsSync(dist) ? readdirSync(dist).join(", ") : "(dist missing)";
  throw new Error(
    `SEA entry not found: expected dist/cli.cjs. Dist contents: ${distEntries}`
  );
}
if (!existsSync(postjectBinary)) {
  throw new Error("postject binary not found. Run `npm install` before packaging.");
}

rmSync(releaseDir, { recursive: true, force: true });
mkdirSync(releaseDir, { recursive: true });

writeFileSync(
  seaConfig,
  JSON.stringify(
    {
      main: "./dist/cli.cjs",
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

execFileSync(
  postjectBinary,
  [
    releaseBinary,
    "NODE_SEA_BLOB",
    outputBlob,
    "--sentinel-fuse",
    "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2"
  ],
  { stdio: "inherit", cwd: root }
);

const helpOutput = execFileSync(releaseBinary, ["--help"], {
  cwd: root,
  encoding: "utf8"
});

if (!helpOutput.includes("Memovyn v0.2.0")) {
  throw new Error(
    "SEA packaging validation failed: packaged executable did not boot Memovyn correctly."
  );
}
