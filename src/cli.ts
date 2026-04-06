#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { argv, env, execArgv, execPath, exit, stdin, stdout } from "node:process";
import { createInterface } from "node:readline/promises";

import { ensureConfig, loadConfig } from "./config.ts";

ensureExperimentalSqlite();

async function main(): Promise<void> {
  const [command = "help", ...rest] = argv.slice(2);

  if (command === "help" || command === "--help" || command === "-h") {
    stdout.write(helpText);
    return;
  }

  const [{ Memovyn }, { serveHttp }] = await Promise.all([
    import("./app.ts"),
    import("./http.ts")
  ]);
  const config = loadConfig();
  ensureConfig(config);
  const app = new Memovyn(config);

  switch (command) {
    case "serve":
      await serveHttp(app, readFlag(rest, "--bind") ?? "127.0.0.1:7761");
      return;
    case "mcp-stdio":
      await serveMcpStdio(app);
      return;
    case "add":
      print(
        await app.addMemory({
          projectId: requiredArg(rest, 0, "project_id"),
          content: requiredArg(rest, 1, "content")
        })
      );
      return;
    case "search":
      print(
        app.searchMemories({
          projectId: requiredArg(rest, 0, "project_id"),
          query: requiredArg(rest, 1, "query"),
          limit: Number(readFlag(rest, "--limit") ?? "8")
        })
      );
      return;
    case "context":
      print(app.getProjectContext(requiredArg(rest, 0, "project_id")));
      return;
    case "reflect":
      print(
        await app.reflectMemory({
          projectId: requiredArg(rest, 0, "project_id"),
          taskResult: requiredArg(rest, 1, "task_result"),
          outcome: (readFlag(rest, "--outcome") ?? "partial") as never
        })
      );
      return;
    case "feedback":
      print(
        app.feedbackMemory({
          memoryId: requiredArg(rest, 0, "memory_id"),
          outcome: (readFlag(rest, "--outcome") ?? "partial") as never,
          repeatedMistake: readFlag(rest, "--repeated-mistake") === "true",
          weight: Number(readFlag(rest, "--weight") ?? "1"),
          crossProjectInfluence: true
        })
      );
      return;
    case "archive":
      print(app.archiveMemory({ memoryId: requiredArg(rest, 0, "memory_id") }));
      return;
    case "projects":
      print(app.listProjects());
      return;
    case "analytics": {
      const projectId = requiredArg(rest, 0, "project_id");
      if (rest.includes("--csv")) {
        stdout.write(app.analyticsCsv(projectId));
      } else if (rest.includes("--markdown")) {
        stdout.write(app.analyticsMarkdown(projectId));
      } else {
        print(app.analytics(projectId));
      }
      return;
    }
    case "inspect":
      print(app.inspectMemory(requiredArg(rest, 0, "memory_id")));
      return;
    case "export":
      app.exportProject(requiredArg(rest, 0, "project_id"), requiredArg(rest, 1, "output"));
      stdout.write(`exported ${requiredArg(rest, 0, "project_id")} to ${requiredArg(rest, 1, "output")}\n`);
      return;
    case "import":
      stdout.write(`imported ${app.importBundle(requiredArg(rest, 0, "input"))} memories\n`);
      return;
    case "benchmark":
      stdout.write(
        `${await app.benchmark(
          requiredArg(rest, 0, "project_id"),
          Number(readFlag(rest, "--memories") ?? "5000"),
          readFlag(rest, "--query") ?? "sqlite bm25 dashboard"
        )}\n`
      );
      return;
    default:
      stdout.write(helpText);
  }
}

async function serveMcpStdio(app: Awaited<ReturnType<typeof loadRuntimeApp>>): Promise<void> {
  const { handleMcpRequest } = await import("./mcp.ts");
  const rl = createInterface({ input: stdin, output: stdout, terminal: false });
  for await (const line of rl) {
    if (!line.trim()) continue;
    const request = JSON.parse(line);
    stdout.write(`${JSON.stringify(await handleMcpRequest(app, request))}\n`);
  }
}

function ensureExperimentalSqlite(): void {
  if (execArgv.includes("--experimental-sqlite") || env.MEMOVYN_SQLITE_BOOTSTRAPPED === "1") {
    return;
  }

  const result = spawnSync(
    execPath,
    ["--experimental-sqlite", ...execArgv, ...argv.slice(1)],
    {
      stdio: "inherit",
      env: {
        ...env,
        MEMOVYN_SQLITE_BOOTSTRAPPED: "1"
      }
    }
  );

  if (typeof result.status === "number") {
    exit(result.status);
  }
  exit(1);
}

type RuntimeAppModule = typeof import("./app.ts");
async function loadRuntimeApp(): Promise<InstanceType<RuntimeAppModule["Memovyn"]>> {
  const { Memovyn } = await import("./app.ts");
  const config = loadConfig();
  ensureConfig(config);
  return new Memovyn(config);
}

function readFlag(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  if (index === -1) return undefined;
  return args[index + 1];
}

function requiredArg(args: string[], index: number, label: string): string {
  const value = args[index];
  if (!value) {
    throw new Error(`missing required argument: ${label}`);
  }
  return value;
}

function print(value: unknown): void {
  stdout.write(`${JSON.stringify(value, null, 2)}\n`);
}

const helpText = `Memovyn v0.2.0

Commands:
  serve --bind 127.0.0.1:7761
  mcp-stdio
  add <project_id> <content>
  search <project_id> <query> [--limit N]
  context <project_id>
  reflect <project_id> <task_result> --outcome <success|failure|regression|partial>
  feedback <memory_id> --outcome <success|failure|regression|partial> [--weight N]
  archive <memory_id>
  projects
  analytics <project_id> [--csv|--markdown]
  inspect <memory_id>
  export <project_id> <output>
  import <input>
  benchmark <project_id> [--memories N] [--query "..."]
`;

void main().catch((error) => {
  stdout.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
