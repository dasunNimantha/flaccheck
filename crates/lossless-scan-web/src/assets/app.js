const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => [...document.querySelectorAll(sel)];

const MODE_DESC = {
  fast: "Spectral cutoff + hi-res checks — fastest, good for quick library sweeps",
  balanced: "Adds quantization + artifact detectors on suspects — recommended default",
  max: "Exhaustive search on every file — slowest, highest detection depth",
};

const TIER_NAMES = {
  spectral: "Tier 1 · Spectral",
  quant: "Tier 2 · Quantization",
  artifacts: "Tier 3 · Artifacts",
  hires: "Tier 4 · Hi-res",
  abstention: "Tier 5 · Abstention",
  ml: "ML refinement",
};

const VERDICT_ORDER = { TRANSCODED: 0, SUSPICIOUS: 1, INCONCLUSIVE: 2, GENUINE: 3 };

let fileQueue = [];
let lastReport = null;
let lastStats = null;
let activeFilter = "all";

const dropzone = $("#dropzone");
const fileInput = $("#file-input");
const queueSection = $("#queue-section");
const queueList = $("#queue-list");
const queueCount = $("#queue-count");
const btnScan = $("#btn-scan");
const btnClear = $("#btn-clear-queue");
const emptyState = $("#empty-state");
const loading = $("#loading");
const resultsPanel = $("#results-panel");
const resultsEl = $("#results");
const issuesEl = $("#issues");
const issuesList = $("#issues-list");
const alertBanner = $("#alert-banner");

function formatBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDuration(secs) {
  if (!secs) return "";
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function getMode() {
  const checked = $('input[name="mode"]:checked');
  return checked ? checked.value : "balanced";
}

function updateModeDesc() {
  $("#mode-desc").textContent = MODE_DESC[getMode()] || "";
}

function isAudioFile(file) {
  const ext = file.name.split(".").pop()?.toLowerCase() || "";
  return ["flac", "wav", "wave", "mp3", "m4a", "aac", "ogg", "opus", "aiff", "aif", "alac", "ape", "wv"].includes(ext)
    || file.type.startsWith("audio/");
}

function addToQueue(files) {
  const incoming = [...files].filter(isAudioFile);
  if (!incoming.length) return;

  for (const f of incoming) {
    if (!fileQueue.some((q) => q.name === f.name && q.size === f.size)) {
      fileQueue.push(f);
    }
  }
  renderQueue();
}

function removeFromQueue(index) {
  fileQueue.splice(index, 1);
  renderQueue();
}

function clearQueue() {
  fileQueue = [];
  renderQueue();
}

function renderQueue() {
  if (!fileQueue.length) {
    queueSection.classList.add("hidden");
    btnScan.disabled = true;
    return;
  }

  queueSection.classList.remove("hidden");
  btnScan.disabled = false;
  queueCount.textContent = `${fileQueue.length} file${fileQueue.length !== 1 ? "s" : ""}`;
  btnScan.querySelector(".btn-label").textContent =
    fileQueue.length === 1 ? "Analyze 1 file" : `Analyze ${fileQueue.length} files`;

  queueList.innerHTML = "";
  fileQueue.forEach((f, i) => {
    const li = document.createElement("li");
    li.className = "queue-item";
    li.innerHTML = `
      <span class="queue-item-name" title="${escapeHtml(f.name)}">${escapeHtml(f.name)}</span>
      <span class="queue-item-size">${formatBytes(f.size)}</span>
      <button type="button" class="queue-item-remove" aria-label="Remove ${escapeHtml(f.name)}">×</button>
    `;
    li.querySelector(".queue-item-remove").addEventListener("click", (e) => {
      e.stopPropagation();
      removeFromQueue(i);
    });
    queueList.appendChild(li);
  });
}

function escapeHtml(s) {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}

function verdictClass(v) {
  return v || "INCONCLUSIVE";
}

function isFlagged(v) {
  return v === "TRANSCODED" || v === "SUSPICIOUS";
}

function formatConfidence(c) {
  if (c == null || Number.isNaN(c)) return "—";
  return `${Math.round(c * 100)}%`;
}

function groupEvidence(evidence) {
  const groups = {};
  for (const e of evidence || []) {
    const tier = TIER_NAMES[e.detector] || e.detector;
    if (!groups[tier]) groups[tier] = [];
    groups[tier].push(e);
  }
  for (const tier of Object.keys(groups)) {
    groups[tier].sort((a, b) => (b.value * b.weight) - (a.value * b.weight));
  }
  return groups;
}

function shouldShowEvidence(e) {
  return e.weight > 0
    || ["brick_wall", "early_rolloff", "padded_depth", "upsampled", "classical_prob", "onnx_borderline", "ml_abstain"].includes(e.signal);
}

function renderEvidenceBody(r) {
  const explain = $("#explain").checked;
  if (!explain || !r.evidence?.length) {
    return '<p class="no-results-msg">Enable "Show detector evidence" to see per-tier signals.</p>';
  }

  const groups = groupEvidence(r.evidence.filter(shouldShowEvidence));
  let html = '<div class="evidence-tiers">';
  for (const [tier, items] of Object.entries(groups)) {
    html += `<div class="evidence-tier"><div class="evidence-tier-title">${escapeHtml(tier)}</div><ul class="evidence-list">`;
    for (const e of items) {
      html += `<li>
        <span class="signal">${escapeHtml(e.detector)} · ${escapeHtml(e.signal)}</span>
        <span class="val">${Number(e.value).toPrecision(3)}</span>
        <span class="note">${escapeHtml(e.note || "")}${e.weight ? ` (weight ${e.weight})` : ""}</span>
      </li>`;
    }
    html += "</ul></div>";
  }
  html += "</div>";
  return html;
}

function renderResultCard(r) {
  const v = verdictClass(r.transcode_verdict);
  const flagged = isFlagged(v);
  const hiresWarn = ["UPSAMPLED", "PADDED_DEPTH"].includes(r.hires_verdict);
  const cardClass = flagged ? "flagged" : v === "GENUINE" ? "genuine-card" : v === "INCONCLUSIVE" ? "inconclusive-card" : "";

  const card = document.createElement("article");
  card.className = `result-card ${cardClass}`;
  card.dataset.verdict = v;
  card.dataset.name = r.path.toLowerCase();
  card.dataset.confidence = r.confidence;

  const metaParts = [
    `${r.sample_rate} Hz`,
    `${r.channels} ch`,
    r.bits_per_sample ? `${r.bits_per_sample}-bit` : null,
    r.duration_secs ? formatDuration(r.duration_secs) : null,
    `HF ${r.spectral_info_score?.toFixed(3) ?? "—"}`,
    r.codec_guess ? `~${r.codec_guess}` : null,
    r.est_source_bitrate_kbps ? `~${r.est_source_bitrate_kbps} kbps` : null,
  ].filter(Boolean);

  card.innerHTML = `
    <div class="result-header" role="button" tabindex="0" aria-expanded="false">
      <div>
        <div class="file-name">${escapeHtml(r.path)}</div>
        <div class="file-meta">${metaParts.map(escapeHtml).join(" · ")}</div>
        <div class="confidence-bar-wrap">
          <div class="confidence-bar"><div class="confidence-fill ${v}" style="width:${Math.round((r.confidence || 0) * 100)}%"></div></div>
          <span class="confidence-label">${formatConfidence(r.confidence)} confidence</span>
        </div>
      </div>
      <div class="badges">
        <span class="chevron" aria-hidden="true">▶</span>
        <span class="badge ${v}">${v}</span>
        <span class="badge ${hiresWarn ? "hires" : "hires-ok"}">${r.hires_verdict?.replace(/_/g, " ") || "—"}</span>
      </div>
    </div>
    <div class="result-body">${renderEvidenceBody(r)}</div>
  `;

  const header = card.querySelector(".result-header");
  const toggle = () => {
    const expanded = card.classList.toggle("expanded");
    header.setAttribute("aria-expanded", expanded);
  };
  header.addEventListener("click", toggle);
  header.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(); }
  });

  return card;
}

function sortResults(results) {
  const sort = $("#sort").value;
  const copy = [...results];
  switch (sort) {
    case "confidence-desc":
      return copy.sort((a, b) => b.confidence - a.confidence);
    case "confidence-asc":
      return copy.sort((a, b) => a.confidence - b.confidence);
    case "name":
      return copy.sort((a, b) => a.path.localeCompare(b.path));
    default:
      return copy.sort((a, b) =>
        (VERDICT_ORDER[a.transcode_verdict] ?? 9) - (VERDICT_ORDER[b.transcode_verdict] ?? 9)
        || b.confidence - a.confidence
      );
  }
}

function applyFilters() {
  if (!lastReport) return;
  const query = $("#search").value.toLowerCase().trim();
  const cards = resultsEl.querySelectorAll(".result-card");
  let visible = 0;

  cards.forEach((card) => {
    const v = card.dataset.verdict;
    const name = card.dataset.name;
    let show = true;

    if (activeFilter === "flagged" && !isFlagged(v)) show = false;
    else if (activeFilter !== "all" && activeFilter !== "flagged" && v !== activeFilter) show = false;

    if (query && !name.includes(query)) show = false;

    card.classList.toggle("hidden-filter", !show);
    if (show) visible++;
  });

  let msg = resultsEl.querySelector(".no-results-msg");
  if (visible === 0 && cards.length > 0) {
    if (!msg) {
      msg = document.createElement("p");
      msg.className = "no-results-msg";
      resultsEl.appendChild(msg);
    }
    msg.textContent = "No results match your filter.";
    msg.style.display = "block";
  } else if (msg) {
    msg.style.display = "none";
  }
}

function renderReport(data) {
  lastReport = data.report;
  lastStats = data.stats;

  emptyState.classList.add("hidden");
  loading.classList.add("hidden");
  resultsPanel.classList.remove("hidden");

  const s = data.stats;
  $("#stat-total").textContent = s.total;
  $("#stat-flagged").textContent = s.flagged;
  $("#stat-genuine").textContent = s.genuine;
  $("#stat-inconclusive").textContent = s.inconclusive;

  $("#results-meta").textContent =
    `${s.mode} mode · ${s.duration_ms}ms · ${s.transcoded} transcoded · ${s.suspicious} suspicious`;

  if (s.flagged > 0) {
    alertBanner.className = "alert-banner warn";
    alertBanner.textContent = `${s.flagged} file${s.flagged !== 1 ? "s" : ""} may not be genuine lossless — review flagged results below.`;
    alertBanner.classList.remove("hidden");
  } else if (s.total > 0) {
    alertBanner.className = "alert-banner ok";
    alertBanner.textContent = `All ${s.total} file${s.total !== 1 ? "s" : ""} passed — no strong transcode fingerprints detected.`;
    alertBanner.classList.remove("hidden");
  } else {
    alertBanner.classList.add("hidden");
  }

  const sorted = sortResults(data.report.results);
  resultsEl.innerHTML = "";
  for (const r of sorted) {
    resultsEl.appendChild(renderResultCard(r));
  }

  const issues = [...data.report.skipped, ...data.report.errors];
  if (issues.length) {
    issuesList.innerHTML = issues.map((i) => `<li>${escapeHtml(i)}</li>`).join("");
    issuesEl.classList.remove("hidden");
  } else {
    issuesEl.classList.add("hidden");
  }

  applyFilters();
}

function showLoading(n) {
  emptyState.classList.add("hidden");
  resultsPanel.classList.add("hidden");
  loading.classList.remove("hidden");
  $("#loading-text").textContent = `Analyzing ${n} file${n !== 1 ? "s" : ""}…`;
  $("#progress-bar").style.width = "30%";
}

function hideLoading() {
  loading.classList.add("hidden");
  $("#progress-bar").style.width = "100%";
}

async function scanQueue() {
  if (!fileQueue.length) return;

  const mode = getMode();
  const explain = $("#explain").checked;
  const ml = $("#ml").checked;
  const files = [...fileQueue];

  showLoading(files.length);
  btnScan.disabled = true;

  const form = new FormData();
  for (const f of files) form.append("files", f);

  const params = new URLSearchParams({
    mode,
    explain: explain ? "true" : "false",
    ml: ml ? "true" : "false",
  });

  try {
    const res = await fetch(`/api/scan?${params}`, { method: "POST", body: form });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || res.statusText);
    hideLoading();
    renderReport(data);
  } catch (err) {
    hideLoading();
    emptyState.classList.remove("hidden");
    alert(err.message || "Scan failed");
  } finally {
    btnScan.disabled = fileQueue.length === 0;
  }
}

function exportJson() {
  if (!lastReport) return;
  const blob = new Blob([JSON.stringify({ report: lastReport, stats: lastStats }, null, 2)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `lossless-scan-${new Date().toISOString().slice(0, 10)}.json`;
  a.click();
  URL.revokeObjectURL(a.href);
}

// Events
dropzone.addEventListener("dragover", (e) => { e.preventDefault(); dropzone.classList.add("dragover"); });
dropzone.addEventListener("dragleave", () => dropzone.classList.remove("dragover"));
dropzone.addEventListener("drop", (e) => {
  e.preventDefault();
  dropzone.classList.remove("dragover");
  addToQueue(e.dataTransfer.files);
});

fileInput.addEventListener("change", () => {
  addToQueue(fileInput.files);
  fileInput.value = "";
});

dropzone.addEventListener("keydown", (e) => {
  if (e.key === "Enter" || e.key === " ") { e.preventDefault(); fileInput.click(); }
});

btnScan.addEventListener("click", scanQueue);
btnClear.addEventListener("click", clearQueue);
$("#btn-export").addEventListener("click", exportJson);
$("#btn-rescan").addEventListener("click", () => { if (fileQueue.length) scanQueue(); });

$$('input[name="mode"]').forEach((el) => el.addEventListener("change", updateModeDesc));
$("#search").addEventListener("input", applyFilters);
$("#sort").addEventListener("change", () => {
  if (lastReport) {
    const sorted = sortResults(lastReport.results);
    resultsEl.innerHTML = "";
    for (const r of sorted) resultsEl.appendChild(renderResultCard(r));
    applyFilters();
  }
});

$$(".filter-tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    $$(".filter-tab").forEach((t) => t.classList.remove("active"));
    tab.classList.add("active");
    activeFilter = tab.dataset.filter;
    applyFilters();
  });
});

$("#btn-help").addEventListener("click", () => $("#help-dialog").showModal());
$("#help-close").addEventListener("click", () => $("#help-dialog").close());
$("#help-dialog").addEventListener("click", (e) => {
  if (e.target === $("#help-dialog")) $("#help-dialog").close();
});

updateModeDesc();
