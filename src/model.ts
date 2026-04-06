import type { Config } from "./config.ts";
import type { MemoryMetadata, TaxonomyEvolutionSnapshot } from "./types.ts";

export type ModelGuidance = {
  mainCategory?: string;
  boostedLabels: string[];
  languageHint?: string;
  confidence: number;
  avoidPatterns: string[];
  reinforcePatterns: string[];
  notes: string[];
  backend: string;
};

export class ModelHook {
  private readonly config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  async classify(
    content: string,
    metadata: MemoryMetadata,
    evolution: TaxonomyEvolutionSnapshot
  ): Promise<ModelGuidance | null> {
    if (this.config.classifierMode !== "ollama") {
      return null;
    }

    try {
      const response = await fetch(`${this.config.ollamaBaseUrl}/api/generate`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          model: this.config.ollamaModel,
          stream: false,
          format: "json",
          prompt: buildPrompt(content, metadata, evolution)
        }),
        signal: AbortSignal.timeout(this.config.ollamaTimeoutMs)
      });
      if (!response.ok) {
        return null;
      }
      const envelope = (await response.json()) as { response?: string };
      const payload = envelope.response ? JSON.parse(envelope.response) : envelope;
      return {
        mainCategory: payload.main_category,
        boostedLabels: Array.isArray(payload.multi_labels) ? payload.multi_labels : [],
        languageHint: payload.language_hint,
        confidence: clampNumber(payload.confidence, 0, 1),
        avoidPatterns: Array.isArray(payload.avoid_patterns) ? payload.avoid_patterns : [],
        reinforcePatterns: Array.isArray(payload.reinforce_patterns)
          ? payload.reinforce_patterns
          : [],
        notes: Array.isArray(payload.notes) ? payload.notes : [],
        backend: "ollama"
      };
    } catch {
      return null;
    }
  }
}

function buildPrompt(
  content: string,
  metadata: MemoryMetadata,
  evolution: TaxonomyEvolutionSnapshot
): string {
  return [
    "You are Memovyn_0.1B, a tiny classifier for an AI coding memory framework.",
    "Return strict JSON with keys: main_category, multi_labels, language_hint, confidence, avoid_patterns, reinforce_patterns, notes.",
    `Project priors: ${evolution.priorLabels.join(", ")}`,
    `Reinforced priors: ${evolution.reinforcedLabels.join(", ")}`,
    `Solidified priors: ${evolution.solidifiedPriors.join(", ")}`,
    `Avoid patterns: ${evolution.avoidPatterns.join(", ")}`,
    `Metadata language: ${metadata.language ?? "unknown"}`,
    `Paths: ${metadata.paths.join(", ")}`,
    "Content:",
    content
  ].join("\n");
}

function clampNumber(value: unknown, min: number, max: number): number {
  const numeric = typeof value === "number" ? value : 0;
  return Math.max(min, Math.min(max, numeric));
}
