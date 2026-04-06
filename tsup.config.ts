import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/cli.ts"],
  format: ["esm"],
  platform: "node",
  target: "node24",
  dts: true,
  sourcemap: true,
  splitting: false,
  clean: true,
  outDir: "dist",
  minify: false,
  treeshake: true
});
