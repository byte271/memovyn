import type { Memovyn } from "./app.ts";
import type { FeedbackInput, ReflectionInput, SearchInput } from "./types.ts";

type JsonRpcRequest = {
  jsonrpc?: string;
  id?: unknown;
  method: string;
  params?: unknown;
};

export async function handleMcpRequest(app: Memovyn, request: JsonRpcRequest) {
  try {
    if (request.method === "initialize") {
      return ok(request.id, {
        protocolVersion: "2025-11-05",
        serverInfo: { name: "memovyn", version: "0.2.0" },
        capabilities: {
          tools: { listChanged: false },
          logging: {},
          resources: {},
          prompts: {}
        }
      });
    }
    if (request.method === "tools/list") {
      return ok(request.id, { tools: toolList });
    }
    if (request.method === "tools/call") {
      const params = (request.params ?? {}) as { name?: string; arguments?: Record<string, unknown> };
      const name = params.name;
      const args = params.arguments ?? {};
      if (name === "add_memory") {
        const memory = await app.addMemory({
          projectId: String(args.project_id),
          content: String(args.content),
          metadata: (args.metadata as object | undefined) as never,
          kind: (args.kind as never) ?? "observation"
        });
        return ok(request.id, { content: renderToolContent({ memory }) });
      }
      if (name === "search_memories") {
        const response = app.searchMemories({
          projectId: String(args.project_id),
          query: String(args.query),
          limit: Number(args.limit ?? 10),
          filters: (args.filters as object | undefined) as never
        });
        return ok(request.id, { content: renderToolContent(response) });
      }
      if (name === "get_project_context") {
        const response = app.getProjectContext(String(args.project_id));
        return ok(request.id, { content: renderToolContent(response) });
      }
      if (name === "reflect_memory") {
        const response = await app.reflectMemory({
          projectId: String(args.project_id),
          taskResult: String(args.task_result),
          outcome: args.outcome as ReflectionInput["outcome"],
          metadata: (args.metadata as object | undefined) as never
        });
        return ok(request.id, { content: renderToolContent(response) });
      }
      if (name === "feedback_memory") {
        const response = app.feedbackMemory({
          memoryId: String(args.memory_id),
          outcome: args.outcome as FeedbackInput["outcome"],
          repeatedMistake: Boolean(args.repeated_mistake),
          weight: Number(args.weight ?? 1),
          crossProjectInfluence: args.cross_project_influence !== false,
          avoidPatterns: Array.isArray(args.avoid_patterns) ? args.avoid_patterns.map(String) : [],
          note: args.note ? String(args.note) : undefined
        });
        return ok(request.id, { content: renderToolContent(response) });
      }
      if (name === "archive_memory") {
        const response = app.archiveMemory({ memoryId: String(args.memory_id) });
        return ok(request.id, { content: renderToolContent(response) });
      }
      if (name === "get_project_analytics") {
        const response = app.analytics(String(args.project_id));
        return ok(request.id, { content: renderToolContent(response) });
      }
      return error(request.id, -32601, `unknown tool ${String(name)}`);
    }
    if (request.method === "ping") {
      return ok(request.id, { ok: true });
    }
    return error(request.id, -32601, `unknown method ${request.method}`);
  } catch (cause) {
    return error(request.id, -32000, cause instanceof Error ? cause.message : String(cause));
  }
}

function renderToolContent(payload: unknown) {
  const value = payload as Record<string, unknown>;
  const content = [
    {
      type: "text",
      text: JSON.stringify(payload, null, 2)
    },
    {
      type: "resource",
      mimeType: "application/json",
      data: payload
    }
  ];
  if (Array.isArray(value.reconciliationHints)) {
    content.push({
      type: "resource",
      mimeType: "application/vnd.memovyn.hints+json",
      data: {
        hints: value.reconciliationHints,
        actions: [
          { id: "inspect", label: "Inspect" },
          { id: "reinforce", label: "Reinforce" },
          { id: "archive", label: "Archive" }
        ]
      }
    });
  }
  if (value.interactivePrompt) {
    content.push({
      type: "resource",
      mimeType: "application/vnd.memovyn.actions+json",
      data: value.interactivePrompt
    });
  }
  return content;
}

function ok(id: unknown, result: unknown) {
  return { jsonrpc: "2.0", id, result };
}

function error(id: unknown, code: number, message: string) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

const toolList = [
  {
    name: "add_memory",
    description: "Add a project-scoped permanent memory and classify it with Memovyn's algorithmic engine, optionally augmented by Memovyn_0.1B via Ollama.",
    inputSchema: { type: "object", required: ["project_id", "content"], properties: { project_id: { type: "string" }, content: { type: "string" }, metadata: { type: "object" }, kind: { type: "string" } } }
  },
  {
    name: "search_memories",
    description: "Search a project's memories with progressive disclosure output across index, summary, timeline, and detail layers.",
    inputSchema: { type: "object", required: ["project_id", "query"], properties: { project_id: { type: "string" }, query: { type: "string" }, limit: { type: "integer" }, filters: { type: "object" } } }
  },
  {
    name: "get_project_context",
    description: "Return ready-to-inject project context, taxonomy summary, relation graph summary, and debugging notes.",
    inputSchema: { type: "object", required: ["project_id"], properties: { project_id: { type: "string" } } }
  },
  {
    name: "reflect_memory",
    description: "Reflect on a task result, reinforce good outcomes, surface avoid-patterns, and return interactive save confirmation metadata plus action hints.",
    inputSchema: { type: "object", required: ["project_id", "task_result", "outcome"], properties: { project_id: { type: "string" }, task_result: { type: "string" }, outcome: { type: "string" }, metadata: { type: "object" } } }
  },
  {
    name: "feedback_memory",
    description: "Apply explicit success or failure feedback so Memovyn can reinforce good patterns, punish repeated mistakes, and guide the Lead Agent.",
    inputSchema: { type: "object", required: ["memory_id", "outcome"], properties: { memory_id: { type: "string" }, outcome: { type: "string" }, repeated_mistake: { type: "boolean" }, weight: { type: "number" }, cross_project_influence: { type: "boolean" }, avoid_patterns: { type: "array", items: { type: "string" } }, note: { type: "string" } } }
  },
  {
    name: "archive_memory",
    description: "Archive a memory so it leaves active retrieval while remaining inspectable and versioned.",
    inputSchema: { type: "object", required: ["memory_id"], properties: { memory_id: { type: "string" } } }
  },
  {
    name: "get_project_analytics",
    description: "Return analytics showing memory health, learning impact, token savings, and evolution trends.",
    inputSchema: { type: "object", required: ["project_id"], properties: { project_id: { type: "string" } } }
  }
] satisfies unknown[];
