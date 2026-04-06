import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";

export type ClassifierMode = "algorithm" | "ollama";
export type ForgettingPolicy = "off" | "conservative" | "balanced" | "aggressive";

export type Config = {
  dataDir: string;
  databasePath: string;
  classifierMode: ClassifierMode;
  ollamaBaseUrl: string;
  ollamaModel: string;
  ollamaTimeoutMs: number;
  forgettingPolicy: ForgettingPolicy;
};

export function loadConfig(): Config {
  const dataDir = process.env.MEMOVYN_DATA_DIR ?? ".memovyn";
  return {
    dataDir,
    databasePath: process.env.MEMOVYN_DATABASE_PATH ?? join(dataDir, "memovyn.sqlite3"),
    classifierMode: parseClassifierMode(process.env.MEMOVYN_CLASSIFIER_MODE),
    ollamaBaseUrl: process.env.MEMOVYN_OLLAMA_BASE_URL ?? "http://127.0.0.1:11434",
    ollamaModel: process.env.MEMOVYN_OLLAMA_MODEL ?? "memovyn_0.1b",
    ollamaTimeoutMs: Number(process.env.MEMOVYN_OLLAMA_TIMEOUT_MS ?? "350"),
    forgettingPolicy: parseForgettingPolicy(process.env.MEMOVYN_FORGETTING_POLICY)
  };
}

export function ensureConfig(config: Config): void {
  mkdirSync(config.dataDir, { recursive: true });
  mkdirSync(dirname(config.databasePath), { recursive: true });
}

export function parseClassifierMode(value?: string): ClassifierMode {
  return value?.trim().toLowerCase() === "ollama" ? "ollama" : "algorithm";
}

export function parseForgettingPolicy(value?: string): ForgettingPolicy {
  const normalized = value?.trim().toLowerCase();
  if (normalized === "off") return "off";
  if (normalized === "conservative") return "conservative";
  if (normalized === "aggressive") return "aggressive";
  return "balanced";
}
