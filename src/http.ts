import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { extname } from "node:path";

import type { Memovyn } from "./app.ts";
import { handleMcpRequest } from "./mcp.ts";

const appCss = readFileSync(new URL("../static/app.css", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");

export async function serveHttp(app: Memovyn, bind: string): Promise<void> {
  const [host, portText] = bind.split(":");
  const port = Number(portText);
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "127.0.0.1"}`);
      if (req.method === "GET" && url.pathname === "/") {
        sendHtml(res, renderIndex(app));
        return;
      }
      if (req.method === "GET" && url.pathname.startsWith("/projects/")) {
        const projectId = decodeURIComponent(url.pathname.split("/")[2] ?? "");
        sendHtml(res, renderProject(app, projectId));
        return;
      }
      if (req.method === "GET" && url.pathname === "/static/app.css") {
        sendText(res, 200, "text/css; charset=utf-8", appCss);
        return;
      }
      if (req.method === "GET" && url.pathname === "/static/app.js") {
        sendText(res, 200, "application/javascript; charset=utf-8", appJs);
        return;
      }
      if (req.method === "POST" && url.pathname === "/mcp") {
        const payload = await readJson(req);
        sendJson(res, 200, await handleMcpRequest(app, payload));
        return;
      }
      if (req.method === "GET" && url.pathname === "/api/projects") {
        sendJson(res, 200, { projects: app.listProjects() });
        return;
      }
      if (req.method === "GET" && /\/api\/projects\/[^/]+\/context/.test(url.pathname)) {
        const projectId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        sendJson(res, 200, {
          context: app.getProjectContext(projectId),
          analytics: app.analytics(projectId)
        });
        return;
      }
      if (req.method === "GET" && /\/api\/projects\/[^/]+\/analytics$/.test(url.pathname)) {
        const projectId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        sendJson(res, 200, { analytics: app.analytics(projectId) });
        return;
      }
      if (req.method === "GET" && /\/api\/projects\/[^/]+\/analytics\.csv$/.test(url.pathname)) {
        const projectId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        sendText(res, 200, "text/csv; charset=utf-8", app.analyticsCsv(projectId));
        return;
      }
      if (req.method === "GET" && /\/api\/projects\/[^/]+\/analytics\.md$/.test(url.pathname)) {
        const projectId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        sendText(res, 200, "text/markdown; charset=utf-8", app.analyticsMarkdown(projectId));
        return;
      }
      if (req.method === "GET" && /\/api\/projects\/[^/]+\/memories$/.test(url.pathname)) {
        const projectId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        const offset = Number(url.searchParams.get("offset") ?? "0");
        const limit = Number(url.searchParams.get("limit") ?? "40");
        const response = app.searchMemories({
          projectId,
          query: url.searchParams.get("q") ?? "",
          limit: Math.max(offset + limit, 40),
          filters: {
            includeShared: url.searchParams.get("include_shared") === "true",
            includePrivateNotes: true
          }
        });
        sendJson(res, 200, {
          items: response.detailLayer.slice(offset, offset + limit),
          total: response.totalHits,
          offset,
          limit
        });
        return;
      }
      if (req.method === "GET" && /\/api\/memories\/[^/]+$/.test(url.pathname)) {
        const memoryId = decodeURIComponent(url.pathname.split("/")[3] ?? "");
        sendJson(res, 200, { inspection: app.inspectMemory(memoryId) });
        return;
      }
      if (req.method === "POST" && url.pathname === "/api/memories") {
        sendJson(res, 200, { memory: await app.addMemory(await readJson(req)) });
        return;
      }
      if (req.method === "POST" && url.pathname === "/api/reflect") {
        sendJson(res, 200, { reflection: await app.reflectMemory(await readJson(req)) });
        return;
      }
      if (req.method === "POST" && url.pathname === "/api/feedback") {
        sendJson(res, 200, { feedback: app.feedbackMemory(await readJson(req)) });
        return;
      }
      if (req.method === "POST" && url.pathname === "/api/archive") {
        sendJson(res, 200, { archived: app.archiveMemory(await readJson(req)) });
        return;
      }
      sendText(res, 404, "text/plain; charset=utf-8", "Not found");
    } catch (error) {
      sendText(res, 500, "text/plain; charset=utf-8", error instanceof Error ? error.message : String(error));
    }
  });

  await new Promise<void>((resolvePromise) => {
    server.listen({ host, port }, () => {
      console.log(`Memovyn listening on http://${bind}`);
      resolvePromise();
    });
  });
}

function renderIndex(app: Memovyn): string {
  const cards = app
    .listProjects()
    .map(
      (project) => `
        <a class="project-card" href="/projects/${project.projectId}">
          <div class="project-card__header">
            <strong>${escapeHtml(project.projectId)}</strong>
            <span>${project.memoryCount} memories</span>
          </div>
          <div class="project-card__meta">
            <span>updated ${project.lastUpdatedAt ?? "new"}</span>
            <span>${project.shareScope ? "cross-project" : "project-only"}</span>
          </div>
        </a>`
    )
    .join("");

  return baseHtml(
    "Memovyn",
    `<main class="shell">
      <section class="hero">
        <div class="hero__controls">
          <button id="theme-toggle" class="theme-toggle" type="button">Toggle Theme</button>
        </div>
        <p class="eyebrow">Memovyn v0.2</p>
        <h1>Permanent memory for local-first coding agents.</h1>
        <p class="lede">Taxonomy-native recall, reinforcement learning, strategic forgetting, and hybrid-ready classification in one Node.js 24 codebase.</p>
      </section>
      <section class="panel">
        <div class="panel__header">
          <h2>Projects</h2>
          <span>SQLite-backed, MCP-native</span>
        </div>
        <div class="project-grid">${cards}</div>
      </section>
    </main>`
  );
}

function renderProject(app: Memovyn, projectId: string): string {
  const context = app.getProjectContext(projectId);
  const analytics = app.analytics(projectId);
  return baseHtml(
    `${projectId} · Memovyn`,
    `<body data-project-id="${escapeHtml(projectId)}">
      <main class="shell shell--project">
        <aside class="sidebar panel">
          <div class="sidebar__top">
            <a href="/" class="back-link">Back</a>
            <button id="theme-toggle" class="theme-toggle" type="button">Toggle Theme</button>
          </div>
          <p class="eyebrow">Project memory container</p>
          <h1>${escapeHtml(projectId)}</h1>
          <div class="stats-row">
            <div class="stat-chip"><strong>${analytics.totalMemories}</strong><span>memories</span></div>
            <div class="stat-chip"><strong>${analytics.conflictCount}</strong><span>conflicts</span></div>
            <div class="stat-chip"><strong>${analytics.totalTokenSavings}</strong><span>tokens saved</span></div>
            <div class="stat-chip"><strong>${analytics.sessionTokenSavings}</strong><span>session savings</span></div>
            <div class="stat-chip"><strong>${analytics.memoryHealthScore}</strong><span>health score</span></div>
          </div>
          <pre class="context-card">${escapeHtml(context.readyContext)}</pre>
          <h2>Debug notes</h2>
          <ul class="note-list">${context.debuggingNotes.map((note) => `<li>${escapeHtml(note)}</li>`).join("")}</ul>
        </aside>
        <section class="panel panel--main">
          <div class="panel__header">
            <div>
              <h2>Memory stream</h2>
              <p>Virtualized recall across large project timelines.</p>
            </div>
            <label class="search-box">
              <span>Search</span>
              <input id="memory-search" type="search" placeholder="architecture, bugfix, sqlite, regression...">
            </label>
          </div>
          <section class="panel panel--analytics">
            <div class="panel__header">
              <div>
                <h2>Analytics</h2>
                <p>Visible recall, learning impact, and health for this project.</p>
              </div>
              <div class="export-links">
                <a class="export-link" href="/api/projects/${encodeURIComponent(projectId)}/analytics.csv">Export CSV</a>
                <a class="export-link" href="/api/projects/${encodeURIComponent(projectId)}/analytics.md">Project Memory Health Report</a>
              </div>
            </div>
            <div id="analytics-grid" class="analytics-grid">
              <p class="analytics-placeholder">Loading analytics…</p>
            </div>
          </section>
          <div id="memory-viewport" class="memory-viewport"></div>
          <section id="inspection-drawer" class="inspection-drawer">
            <h3>Memory inspector</h3>
            <p>Click a memory card to inspect taxonomy signals, relations, provenance, and version history.</p>
          </section>
        </section>
      </main>`
  );
}

function baseHtml(title: string, body: string): string {
  return `<!doctype html>
  <html lang="en">
    <head>
      <meta charset="utf-8">
      <meta name="viewport" content="width=device-width, initial-scale=1">
      <title>${escapeHtml(title)}</title>
      <link rel="stylesheet" href="/static/app.css">
    </head>
    <body>
      ${body}
      <script src="/static/app.js"></script>
    </body>
  </html>`;
}

async function readJson(req: NodeJS.ReadableStream): Promise<any> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  const body = Buffer.concat(chunks).toString("utf8");
  return body ? JSON.parse(body) : {};
}

function sendJson(res: import("node:http").ServerResponse, status: number, data: unknown): void {
  sendText(res, status, "application/json; charset=utf-8", JSON.stringify(data));
}

function sendHtml(res: import("node:http").ServerResponse, html: string): void {
  sendText(res, 200, "text/html; charset=utf-8", html);
}

function sendText(
  res: import("node:http").ServerResponse,
  status: number,
  contentType: string,
  body: string
): void {
  res.writeHead(status, { "content-type": contentType });
  res.end(body);
}

function escapeHtml(input: string): string {
  return input.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
