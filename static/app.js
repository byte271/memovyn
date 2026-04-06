const rowHeight = 136;
const overscan = 6;

async function fetchJson(url, options) {
  const response = await fetch(url, options);
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }
  return await response.json();
}

async function fetchMemories(projectId, query, offset, limit) {
  const params = new URLSearchParams({
    q: query ?? "",
    offset: String(offset),
    limit: String(limit),
  });
  return await fetchJson(`/api/projects/${projectId}/memories?${params.toString()}`);
}

async function fetchInspection(memoryId) {
  return await fetchJson(`/api/memories/${memoryId}`);
}

async function fetchAnalytics(projectId) {
  return await fetchJson(`/api/projects/${projectId}/analytics`);
}

async function sendFeedback(memoryId, outcome, repeatedMistake = false) {
  return await fetchJson("/api/feedback", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      memory_id: memoryId,
      outcome,
      repeated_mistake: repeatedMistake,
      weight: outcome === "success" ? 1.15 : 1.0,
      cross_project_influence: true,
      avoid_patterns: [],
      note: "dashboard-feedback",
    }),
  });
}

async function archiveMemory(memoryId) {
  return await fetchJson("/api/archive", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ memory_id: memoryId }),
  });
}

function renderCard(item, index) {
  const labels = (item.labels || [])
    .slice(0, 6)
    .map((label) => `<span class="pill">${label}</span>`)
    .join("");
  const learningScore = (item.reinforcement || 0) - (item.penalty || 0);
  return `
    <article class="memory-card" data-memory-id="${item.memory_id}" style="top:${index * rowHeight}px">
      <h3>${item.headline}</h3>
      <p>${item.summary}</p>
      <div class="memory-card__meta">
        ${labels}
        <span class="pill">${item.main_category}</span>
        <span class="pill">confidence ${item.confidence.toFixed(2)}</span>
        <span class="pill">${item.relation_count} relations</span>
        <span class="pill">score ${item.score.toFixed(2)}</span>
        <span class="pill">learning ${learningScore.toFixed(2)}</span>
      </div>
    </article>
  `;
}

function renderAnalyticsLists(analytics) {
  const mostRecalled = (analytics.most_recalled || [])
    .slice(0, 8)
    .map((item) => `<li><span>${item.headline}<small>${item.summary}</small></span><strong>${item.access_count}</strong></li>`)
    .join("");
  const reinforced = (analytics.most_reinforced || [])
    .slice(0, 6)
    .map((item) => `<li><span>${item.headline}</span><strong>${item.score.toFixed(2)}</strong></li>`)
    .join("");
  const punished = (analytics.most_punished || [])
    .slice(0, 6)
    .map((item) => `<li><span>${item.headline}</span><strong>${item.score.toFixed(2)}</strong></li>`)
    .join("");
  const growth = (analytics.growth || [])
    .slice(-10)
    .map((bucket) => `<li><span>${bucket.bucket}</span><strong>${bucket.memories} memories / ${bucket.recalls} recalls</strong></li>`)
    .join("");
  const heatmap = (analytics.conflict_heatmap || [])
    .slice(-10)
    .map((bucket) => `<li><span>${bucket.bucket}</span><strong>${bucket.conflicts} conflicts / ${bucket.recalls} repeated</strong></li>`)
    .join("");
  const insights = (analytics.behavior_insights || [])
    .map((insight) => `<li>${insight}</li>`)
    .join("");

  return `
    <section class="analytics-card">
      <h3>Behavior insights</h3>
      <ul class="note-list">${insights || `<li>No insights yet</li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Recall leaders</h3>
      <ul class="label-list">${mostRecalled || `<li><span>No recalls yet</span><strong>0</strong></li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Most reinforced</h3>
      <ul class="label-list">${reinforced || `<li><span>No reinforcement yet</span><strong>0</strong></li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Most punished</h3>
      <ul class="label-list">${punished || `<li><span>No punishment yet</span><strong>0</strong></li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Growth over time</h3>
      <ul class="label-list">${growth || `<li><span>No history yet</span><strong>0</strong></li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Conflict heatmap</h3>
      <ul class="label-list">${heatmap || `<li><span>No conflicts yet</span><strong>0</strong></li>`}</ul>
    </section>
    <section class="analytics-card">
      <h3>Token savings</h3>
      <div class="stats-row">
        <div class="stat-chip"><strong>${analytics.total_token_savings}</strong><span>project</span></div>
        <div class="stat-chip"><strong>${analytics.session_token_savings}</strong><span>session</span></div>
        <div class="stat-chip"><strong>${analytics.total_queries}</strong><span>queries</span></div>
        <div class="stat-chip"><strong>${analytics.session_queries}</strong><span>session queries</span></div>
      </div>
    </section>
  `;
}

async function mountProjectView() {
  const projectId = document.body.dataset.projectId;
  const viewport = document.getElementById("memory-viewport");
  const drawer = document.getElementById("inspection-drawer");
  const analyticsGrid = document.getElementById("analytics-grid");
  const reinforcementPanel = document.getElementById("reinforcement-panel");
  if (!projectId || !viewport) return;

  const spacer = document.createElement("div");
  spacer.className = "memory-spacer";
  viewport.appendChild(spacer);

  const search = document.getElementById("memory-search");
  let total = 0;
  let query = "";
  let currentInspectionId = null;

  async function refreshAnalytics() {
    const payload = await fetchAnalytics(projectId);
    const analytics = payload.analytics;
    if (analyticsGrid) {
      analyticsGrid.innerHTML = renderAnalyticsLists(analytics);
    }
    if (reinforcementPanel) {
      const leaders = (analytics.most_reinforced || [])
        .slice(0, 4)
        .map((item) => `<div class="leader-row"><span>${item.headline}</span><strong>${item.score.toFixed(2)}</strong></div>`)
        .join("");
      reinforcementPanel.innerHTML = leaders || `<p class="analytics-placeholder">No reinforcement leaders yet.</p>`;
    }
  }

  async function refreshMemories() {
    const visibleCount = Math.ceil(viewport.clientHeight / rowHeight) + overscan * 2;
    const start = Math.max(0, Math.floor(viewport.scrollTop / rowHeight) - overscan);
    const payload = await fetchMemories(projectId, query, start, visibleCount);
    total = payload.total;
    spacer.style.height = `${Math.max(total, payload.items.length) * rowHeight}px`;
    spacer.innerHTML = payload.items
      .map((item, index) => renderCard(item, start + index))
      .join("");
  }

  async function showInspection(memoryId) {
    const payload = await fetchInspection(memoryId);
    const inspection = payload.inspection;
    currentInspectionId = memoryId;
    if (!inspection || !drawer) {
      drawer.innerHTML = "<h3>Memory inspector</h3><p>No inspection data found.</p>";
      return;
    }

    const versions = (inspection.versions || [])
      .map((version) => `v${version.version} (${version.reinforcement.toFixed(1)}/${version.penalty.toFixed(1)})`)
      .join(", ");
    const learning = inspection.explanation[4] ?? "";

    drawer.innerHTML = `
      <h3>${inspection.memory.headline}</h3>
      <p>${inspection.memory.taxonomy.metadata.summary}</p>
      <p><strong>Dimensions:</strong> ${inspection.explanation[2] ?? ""}</p>
      <p><strong>Relations:</strong> ${inspection.explanation[3] ?? ""}</p>
      <p><strong>Learning:</strong> ${learning}</p>
      <p><strong>Versions:</strong> ${versions || "v1"}</p>
      <div class="feedback-actions">
        <button data-feedback="success">Reinforce</button>
        <button data-feedback="failure">Punish</button>
        <button data-feedback="regression">Mark regression</button>
        <button data-feedback="archive">Archive</button>
      </div>
    `;
  }

  viewport.addEventListener("scroll", () => {
    window.requestAnimationFrame(refreshMemories);
  });

  viewport.addEventListener("click", async (event) => {
    const card = event.target.closest(".memory-card");
    if (!card) return;
    const memoryId = card.dataset.memoryId;
    if (!memoryId) return;
    await showInspection(memoryId);
  });

  drawer?.addEventListener("click", async (event) => {
    const button = event.target.closest("button[data-feedback]");
    if (!button || !currentInspectionId) return;
    const outcome = button.dataset.feedback;
    if (outcome === "archive") {
      await archiveMemory(currentInspectionId);
    } else {
      await sendFeedback(currentInspectionId, outcome, outcome === "regression");
    }
    await Promise.all([showInspection(currentInspectionId), refreshAnalytics(), refreshMemories()]);
  });

  search?.addEventListener("input", () => {
    query = search.value.trim();
    viewport.scrollTop = 0;
    refreshMemories();
  });

  await Promise.all([refreshAnalytics(), refreshMemories()]);
}

mountProjectView().catch((error) => {
  console.error(error);
});
