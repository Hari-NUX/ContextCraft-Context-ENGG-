<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
import { Effect, getCurrentWindow } from "@tauri-apps/api/window";
  import ChatView from "./lib/components/ChatView.svelte";
  import ContextView from "./lib/components/ContextView.svelte";
  import ExperimentView from "./lib/components/ExperimentView.svelte";
  import AnalyticsView from "./lib/components/AnalyticsView.svelte";
  import PromptGeneratorView from "./lib/components/PromptGeneratorView.svelte";
  import NotesView from "./lib/components/NotesView.svelte";
  import { getOllamaStatus, listDocuments, pullOllamaModel as pullModelRequest } from "./lib/services/ollama";
  import type { ConnectionState, OllamaModel, OllamaStatus } from "./lib/types/ollama";
  import type { DocumentMetadata } from "./lib/types/chat";

  let baseUrl = $state("http://localhost:11434");
  let connection = $state<ConnectionState>("checking");
  let status = $state<OllamaStatus | null>(null);
  let errorMessage = $state("");
  let isRefreshing = $state(false);
  let lastChecked = $state<Date | null>(null);
  let libraryDocuments = $state<DocumentMetadata[]>([]);
  let currentPage = $state<"dashboard" | "playground" | "context" | "notes" | "analytics" | "prompt">("dashboard");
  let playgroundTab = $state<"chat" | "experiments">("chat");
  let modelNameToPull = $state("");
  let activePullId = $state("");
  let pullProgress = $state<{ status: string; total?: number; completed?: number; done: boolean } | null>(null);
  let pullNotice = $state("");
  let isPullingModel = $state(false);
  let isFileDropActive = $state(false);
  let selectedAccent = $state("#159e9a");
  let customAccent = $state("#159e9a");
  let pullPercent = $derived(pullProgress?.done ? 100 : pullProgress?.total ? Math.min(100, Math.round(((pullProgress.completed ?? 0) / pullProgress.total) * 100)) : 0);
  const connected = $derived(connection === "connected");

  const runningNames = $derived(new Set(status?.runningModels ?? []));
  const accentPresets = [
    { name: "Purple", color: "#a457d6" },
    { name: "Violet", color: "#7b4ce2" },
    { name: "Indigo", color: "#5559d8" },
    { name: "Ocean blue", color: "#287fce" },
    { name: "Sky blue", color: "#168fce" },
    { name: "Teal", color: "#159e9a" },
    { name: "Emerald", color: "#21966a" },
    { name: "Rose", color: "#d14b83" },
    { name: "Coral", color: "#d35d55" },
    { name: "Amber", color: "#bd8128" },
  ];

  function blendHex(hex: string, target: string, amount: number) {
    const source = hex.replace("#", "");
    const destination = target.replace("#", "");
    const channels = [0, 2, 4].map((offset) => {
      const from = Number.parseInt(source.slice(offset, offset + 2), 16);
      const to = Number.parseInt(destination.slice(offset, offset + 2), 16);
      return Math.round(from + (to - from) * amount).toString(16).padStart(2, "0");
    });
    return "#" + channels.join("");
  }

  function setAccent(color: string, persist = true) {
    if (!/^#[0-9a-f]{6}$/i.test(color)) return;
    selectedAccent = color.toLowerCase();
    customAccent = color.toLowerCase();
    const r = Number.parseInt(color.slice(1, 3), 16);
    const g = Number.parseInt(color.slice(3, 5), 16);
    const b = Number.parseInt(color.slice(5, 7), 16);
    document.documentElement.style.setProperty("--theme-accent", color);
    document.documentElement.style.setProperty("--theme-accent-bright", blendHex(color, "#ffffff", 0.38));
    document.documentElement.style.setProperty("--theme-accent-dark", blendHex(color, "#20242b", 0.55));
    document.documentElement.style.setProperty("--theme-accent-rgb", `${r}, ${g}, ${b}`);
    document.documentElement.style.setProperty("--purple", color);
    document.documentElement.style.setProperty("--purple-bright", blendHex(color, "#ffffff", 0.38));
    if (persist) {
      try { localStorage.setItem("contextcraft-accent", color); } catch { /* The in-memory theme still applies. */ }
    }
    try { void getCurrentWindow().setEffects({ effects: [Effect.Acrylic], color: color + "3d" }).catch(() => {}); } catch { /* Browser preview has no native window effect. */ }
  }

  onMount(() => {
    let disposed = false;
    let stopPullListener: (() => void) | undefined;
    let fileDragDepth = 0;
    const closeMenusOnOutsideClick = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      document.querySelectorAll<HTMLDetailsElement>(".theme-menu[open], .global-library-menu[open], .chat-settings[open]").forEach((menu) => {
        if (!menu.contains(target)) menu.open = false;
      });
    };
    try {
      const savedAccent = localStorage.getItem("contextcraft-accent");
      if (savedAccent && /^#[0-9a-f]{6}$/i.test(savedAccent)) setAccent(savedAccent, false);
      else setAccent("#159e9a", false);
    } catch { setAccent("#159e9a", false); }
    void refreshStatus();
    void refreshLibrary();
    void listen<{ requestId: string; status: string; total?: number; completed?: number; done: boolean }>("ollama-pull-progress", (event) => {
      if (event.payload.requestId !== activePullId) return;
      pullProgress = event.payload;
      if (event.payload.done) pullNotice = `Installed ${modelNameToPull.trim()}. Refreshing your local model list…`;
    }).then((stop) => { if (disposed) stop(); else stopPullListener = stop; });
    const onLibraryUpdate = () => { void refreshLibrary(); };
    const onRequestFilePicker = () => openFilePicker();
    const onRequestOpenDocument = (event: Event) => {
      const id = (event as CustomEvent<{ id?: string }>).detail?.id;
      if (id) openLibraryDocument(id);
    };
    const onFileDragEnter = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      fileDragDepth += 1;
      isFileDropActive = true;
    };
    const onFileDragOver = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
    };
    const onFileDragLeave = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      fileDragDepth = Math.max(0, fileDragDepth - 1);
      if (fileDragDepth === 0) isFileDropActive = false;
    };
    const onFileDrop = (event: DragEvent) => {
      const files = Array.from(event.dataTransfer?.files ?? []);
      if (!files.length) return;
      fileDragDepth = 0;
      isFileDropActive = false;
      // Let the Notes workspace assign dropped files to its active folder.
      if (currentPage === "notes") return;
      event.preventDefault();
      event.stopPropagation();
      if (currentPage === "playground") {
        window.dispatchEvent(new CustomEvent("contextcraft:import-files", { detail: { files } }));
        return;
      }
      currentPage = "context";
      window.setTimeout(() => window.dispatchEvent(new CustomEvent("contextcraft:import-files", { detail: { files } })), 100);
    };
    window.addEventListener("contextcraft:documents-updated", onLibraryUpdate);
    document.addEventListener("pointerdown", closeMenusOnOutsideClick);
    window.addEventListener("contextcraft:request-file-picker", onRequestFilePicker);
    window.addEventListener("contextcraft:request-open-document", onRequestOpenDocument);
    window.addEventListener("dragenter", onFileDragEnter, true);
    window.addEventListener("dragover", onFileDragOver, true);
    window.addEventListener("dragleave", onFileDragLeave, true);
    window.addEventListener("drop", onFileDrop);
    return () => {
      disposed = true;
      stopPullListener?.();
      window.removeEventListener("contextcraft:documents-updated", onLibraryUpdate);
      document.removeEventListener("pointerdown", closeMenusOnOutsideClick);
      window.removeEventListener("contextcraft:request-file-picker", onRequestFilePicker);
      window.removeEventListener("contextcraft:request-open-document", onRequestOpenDocument);
      window.removeEventListener("dragenter", onFileDragEnter, true);
      window.removeEventListener("dragover", onFileDragOver, true);
      window.removeEventListener("dragleave", onFileDragLeave, true);
      window.removeEventListener("drop", onFileDrop);
    };
  });

  async function pullModel(event: SubmitEvent) {
    event.preventDefault();
    const requestedName = modelNameToPull.trim();
    if (!requestedName || !connected || isPullingModel) return;
    activePullId = crypto.randomUUID();
    pullProgress = { status: "Starting model download…", done: false };
    pullNotice = "";
    isPullingModel = true;
    try {
      await pullModelRequest(activePullId, baseUrl, requestedName);
      pullProgress = { status: "success", done: true };
      pullNotice = `Installed ${requestedName}. The refreshed model list is ready.`;
      modelNameToPull = "";
      await refreshStatus();
    } catch (error) {
      pullProgress = null;
      pullNotice = error instanceof Error ? error.message : String(error);
    } finally {
      isPullingModel = false;
      activePullId = "";
    }
  }

  async function refreshLibrary() {
    try { libraryDocuments = await listDocuments(); }
    catch { libraryDocuments = []; }
  }

  function openFilePicker() {
    currentPage = "context";
    window.setTimeout(() => window.dispatchEvent(new Event("contextcraft:open-file-picker")), 80);
  }

  function openLibraryDocument(id: string) {
    currentPage = "context";
    window.setTimeout(() => window.dispatchEvent(new CustomEvent("contextcraft:open-document", { detail: { id } })), 80);
  }

  async function refreshStatus() {
    isRefreshing = true;
    connection = "checking";
    errorMessage = "";

    try {
      const result = await getOllamaStatus(baseUrl.trim());
      status = result;
      connection = "connected";
      lastChecked = new Date();
    } catch (error) {
      status = null;
      connection = "disconnected";
      errorMessage = error instanceof Error ? error.message : String(error);
      lastChecked = new Date();
    } finally {
      isRefreshing = false;
    }
  }

  function formatSize(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "Size unavailable";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let size = bytes;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit += 1;
    }
    return `${size >= 10 || unit < 2 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
  }

  function modelSubtitle(model: OllamaModel): string {
    return [model.details?.parameter_size, model.details?.quantization_level]
      .filter(Boolean)
      .join(" · ") || model.details?.family || "Local Ollama model";
  }

  function checkedLabel(date: Date | null): string {
    return date
      ? `Updated ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
      : "Not checked yet";
  }
</script>

<svelte:head>
  <title>ContextCraft · Local model dashboard</title>
</svelte:head>

<div class="app-frame">
  <aside class="sidebar" aria-label="Main navigation">
    <div class="brand-mark" aria-label="ContextCraft">C</div>
    <div class="nav-group">
      <button class="nav-item" class:selected={currentPage === "dashboard"} aria-current={currentPage === "dashboard" ? "page" : undefined} onclick={() => (currentPage = "dashboard")} title="Dashboard">
        <span class="nav-icon">⌂</span><span class="nav-label">Dashboard</span>
      </button>
      <button class="nav-item" class:selected={currentPage === "playground"} aria-current={currentPage === "playground" ? "page" : undefined} onclick={() => (currentPage = "playground")} title="Playground">
        <span class="nav-icon">◌</span><span class="nav-label">Playground</span>
      </button>
      <button class="nav-item" class:selected={currentPage === "context"} aria-current={currentPage === "context" ? "page" : undefined} onclick={() => (currentPage = "context")} title="Context workspace">
        <span class="nav-icon">▤</span><span class="nav-label">Context</span>
      </button>
      <button class="nav-item" class:selected={currentPage === "notes"} aria-current={currentPage === "notes" ? "page" : undefined} onclick={() => (currentPage = "notes")} title="Notes workspace">
        <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4.5h12a2 2 0 0 1 2 2v13H7a2 2 0 0 1-2-2z"/><path d="M8 4.5v15M11 9h5M11 12.5h5M11 16h4"/></svg><span class="nav-label">Notes</span>
      </button>
      <button class="nav-item" class:selected={currentPage === "analytics"} aria-current={currentPage === "analytics" ? "page" : undefined} onclick={() => (currentPage = "analytics")} title="Analytics">
        <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 19.5h16"/><rect x="5.5" y="11" width="3.5" height="6.5" rx=".8"/><rect x="10.5" y="7.5" width="3.5" height="10" rx=".8"/><rect x="15.5" y="4" width="3.5" height="13.5" rx=".8"/></svg><span class="nav-label">Analytics</span>
      </button>
      <button class="nav-item" class:selected={currentPage === "prompt"} aria-current={currentPage === "prompt" ? "page" : undefined} onclick={() => (currentPage = "prompt")} title="Prompt Generator">
        <svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="m12 2.8 1.7 6.1 6.1 1.7-6.1 1.7-1.7 6.1-1.7-6.1-6.1-1.7 6.1-1.7z"/><path d="m19 15 .8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z"/></svg><span class="nav-label">Prompt</span>
      </button>
    </div>
    <div class="sidebar-bottom">
      <span class="sidebar-caption">LOCAL FIRST</span>
      <span class="privacy-dot" aria-hidden="true"></span>
    </div>
  </aside>

  <main class="main-area">
    <header class="topbar">
      <div>
        <p class="eyebrow">YOUR AI WORKSPACE</p>
        <h1>{currentPage === "dashboard" ? "Dashboard" : currentPage === "playground" ? "Playground" : currentPage === "context" ? "Context workspace" : currentPage === "notes" ? "Notes" : currentPage === "analytics" ? "Analytics" : "Prompt Generator"}</h1>
      </div>
      <div class="topbar-meta">
        <details class="global-library-menu">
          <summary>Files <span>{libraryDocuments.length}</span></summary>
          <div class="global-library-popover">
            <p class="eyebrow">LOCAL SOURCE LIBRARY</p>
            {#if libraryDocuments.length}
              {#each libraryDocuments as document}
                <button class="global-library-item" onclick={() => openLibraryDocument(document.id)}><span>{document.name}</span><small>{document.pageCount} pages</small></button>
              {/each}
            {:else}<p class="global-library-empty">No files added yet.</p>{/if}
            <button class="global-library-add" onclick={openFilePicker}>＋ Add files</button>
          </div>
        </details>
        <span class="local-chip"><span class="privacy-dot"></span> Running locally</span>
        <details class="theme-menu">
          <summary aria-label="Choose app accent color" title="Appearance"><span class="theme-menu-dot" style={`--theme-swatch:${selectedAccent}`}>◉</span><span>Theme</span></summary>
          <div class="theme-popover">
            <p class="eyebrow">ACCENT COLOR</p>
            <div class="theme-preset-grid">
              {#each accentPresets as preset (preset.name)}
                <button class="theme-preset" class:active={selectedAccent === preset.color} onclick={() => setAccent(preset.color)} aria-label={`Use ${preset.name} theme`} title={preset.name}>
                  <span style={`--theme-swatch:${preset.color}`}></span><small>{preset.name}</small>
                </button>
              {/each}
            </div>
            <label class="theme-custom"><span>Custom color</span><input type="color" bind:value={customAccent} onchange={(event) => setAccent((event.currentTarget as HTMLInputElement).value)} /></label>
            <button class="theme-default-button" onclick={() => setAccent("#159e9a")}>Reset to teal</button>
          </div>
        </details>
        <div class="avatar" aria-label="Local workspace">H</div>
      </div>
    </header>

    <div hidden={currentPage !== "dashboard"}>
    <section class="welcome-row">
      <div>
        <p class="eyebrow accent-text">CHECKPOINT 01</p>
        <h2>Your local models, at a glance.</h2>
        <p class="muted">Connect to Ollama and confirm the models available for your experiments.</p>
      </div>
      <div class="connection-pill {connection}">
        <span class="status-light"></span>
        {connection === "checking" ? "Checking Ollama" : connection === "connected" ? "Ollama connected" : "Ollama offline"}
      </div>
    </section>

    <section class="overview-grid" aria-label="Ollama overview">
      <article class="glass-card service-card">
        <div class="card-heading">
          <div>
            <p class="eyebrow">LOCAL SERVICE</p>
            <h3>Ollama status</h3>
          </div>
          <span class="service-icon">◉</span>
        </div>

        <div class="service-details">
          <div class="detail-row">
            <span class="detail-label">Connection</span>
            <span class="detail-value"><span class="mini-dot {connection}"></span>{connection === "connected" ? "Available" : connection === "checking" ? "Checking…" : "Unavailable"}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Ollama version</span>
            <span class="detail-value">{status?.version ?? "—"}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Installed models</span>
            <span class="detail-value">{status?.models.length ?? "—"}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Currently running</span>
            <span class="detail-value">{status?.runningModels.length ?? "—"}</span>
          </div>
        </div>

        <label class="endpoint-label" for="ollama-url">Ollama address</label>
        <div class="endpoint-control">
          <input id="ollama-url" bind:value={baseUrl} spellcheck="false" onkeydown={(event) => event.key === "Enter" && refreshStatus()} />
          <button class="icon-button" onclick={refreshStatus} disabled={isRefreshing} aria-label="Check Ollama address" title="Check address">↵</button>
        </div>
        <div class="card-footer">
          <span class="muted small-text">{checkedLabel(lastChecked)}</span>
          <button class="secondary-button" onclick={refreshStatus} disabled={isRefreshing}>
            <span class:spin={isRefreshing}>⟳</span>{isRefreshing ? "Checking" : "Refresh"}
          </button>
        </div>
        {#if errorMessage}
          <p class="error-note" role="alert">{errorMessage}</p>
        {/if}
      </article>

      <article class="glass-card model-summary-card">
        <div class="card-heading">
          <div>
            <p class="eyebrow">MODEL LIBRARY</p>
            <h3>Ready for experiments</h3>
          </div>
          <span class="model-count">{status?.models.length ?? "—"}<small>models</small></span>
        </div>
        {#if connection === "connected"}
          <div class="summary-list">
            {#each (status?.models ?? []).slice(0, 4) as model}
              <div class="summary-model">
                <span class="model-orb">✳</span>
                <div class="summary-model-copy"><strong>{model.name}</strong><span>{modelSubtitle(model)}</span></div>
                <span class="running-mark" class:active={runningNames.has(model.name) || runningNames.has(model.model)} title={runningNames.has(model.name) || runningNames.has(model.model) ? "Running" : "Not running"}></span>
              </div>
            {/each}
            {#if (status?.models.length ?? 0) === 0}
              <div class="empty-state"><span class="empty-icon">◇</span><strong>No models found</strong><span>Pull a model in Ollama, then refresh this page.</span></div>
            {/if}
          </div>
          {#if (status?.models.length ?? 0) > 4}
            <p class="muted small-text model-footnote">Showing 4 of {status?.models.length} installed models. All models are available in the next chat checkpoint.</p>
          {/if}
        {:else}
          <div class="empty-state">
            <span class="empty-icon">◇</span>
            <strong>{connection === "checking" ? "Finding your models" : "Could not reach Ollama"}</strong>
            <span>{connection === "checking" ? "Checking the local service at the address shown." : "Make sure Ollama is running, then refresh."}</span>
          </div>
        {/if}
        <div class="summary-footer"><span><span class="running-mark active"></span> Running</span><span><span class="running-mark"></span> Available</span></div>
      </article>
    </section>

    <section class="glass-card model-hub-panel" aria-labelledby="model-hub-title">
      <div class="model-hub-heading">
        <div><p class="eyebrow accent-text">LOCAL MODEL HUB</p><h3 id="model-hub-title">Manage Ollama models</h3><p class="muted">Pull a model by its Ollama library name. Downloads can be several gigabytes, so they start only when you submit a model name.</p></div>
        <span class="model-count">{status?.models.length ?? 0}<small>installed</small></span>
      </div>
      <form class="model-pull-form" onsubmit={pullModel}>
        <label for="ollama-pull-name">MODEL NAME</label>
        <input id="ollama-pull-name" bind:value={modelNameToPull} placeholder="For example: qwen3:0.6b" disabled={!connected || isPullingModel} />
        <button class="primary-button" type="submit" disabled={!connected || isPullingModel || !modelNameToPull.trim()}>{isPullingModel ? "Downloading…" : "Pull model"}<span>↓</span></button>
      </form>
      {#if pullProgress}
        <div class="model-pull-progress" role="status">
          <div><strong>{isPullingModel ? pullProgress.status : "Download complete"}</strong>{#if pullProgress.total}<span>{pullPercent}% · {formatSize(pullProgress.completed ?? 0)} / {formatSize(pullProgress.total)}</span>{/if}</div>
          <progress value={pullPercent} max="100" aria-label="Model download progress"></progress>
        </div>
      {/if}
      {#if pullNotice}<p class="model-pull-notice" class:error-text={!pullProgress?.done && !isPullingModel} role="status">{pullNotice}</p>{/if}
      <div class="model-hub-list-heading"><strong>Installed on this Ollama server</strong><button class="secondary-button" onclick={refreshStatus} disabled={!connected || isRefreshing}>{isRefreshing ? "Refreshing…" : "Refresh list"}</button></div>
      {#if status?.models.length}
        <div class="model-hub-list">{#each status.models as model (model.name)}<article><span class="model-orb">✳</span><div><strong>{model.name}</strong><small>{modelSubtitle(model)} · {formatSize(model.size)}</small></div><span class="running-mark" class:active={runningNames.has(model.name) || runningNames.has(model.model)} title={runningNames.has(model.name) || runningNames.has(model.model) ? "Running" : "Available"}></span></article>{/each}</div>
      {:else}<p class="model-hub-empty">{connected ? "No models installed yet. Enter an Ollama model name above to pull one." : "Connect to Ollama to manage local models."}</p>{/if}
    </section>

    <section class="lower-grid">
      <article class="glass-card next-card">
        <div class="next-icon">✦</div>
        <div>
          <p class="eyebrow accent-text">NEW WORKSPACE</p>
          <h3>Prompt generator</h3>
        <p class="muted">Build copy-ready prompts for another AI tool and get guidance on whether to share a whole document, selected pages, or a focused excerpt.</p>
        </div>
        <button class="coming-soon" onclick={() => (currentPage = "prompt")}>OPEN ↗</button>
      </article>
      <article class="privacy-card">
        <span class="privacy-shield">⬡</span>
        <div><strong>Choose your model provider</strong><p>Ollama conversations stay on your configured local server. If you choose a cloud provider in Playground, prompts and selected context are sent to that provider.</p></div>
      </article>
    </section>
    <footer class="page-footer"><span>ContextCraft</span><span>Local model dashboard</span></footer>
    </div>
    <div hidden={currentPage !== "playground"}>
      <div class="workspace-tabs" role="tablist" aria-label="Playground workspaces">
        <button class:active={playgroundTab === "chat"} role="tab" aria-selected={playgroundTab === "chat"} onclick={() => (playgroundTab = "chat")}>Local chat</button>
        <button class:active={playgroundTab === "experiments"} role="tab" aria-selected={playgroundTab === "experiments"} onclick={() => (playgroundTab = "experiments")}>Comparison lab</button>
      </div>
      <div hidden={playgroundTab !== "chat"}><ChatView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} /></div>
      <div hidden={playgroundTab !== "experiments"}><ExperimentView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} /></div>
    </div>
    <div hidden={currentPage !== "context"}><ContextView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} /></div>
    <div hidden={currentPage !== "notes"}><NotesView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} /></div>
    <div hidden={currentPage !== "analytics"}><AnalyticsView /></div>
    <div hidden={currentPage !== "prompt"}><PromptGeneratorView /></div>
  </main>
  {#if isFileDropActive}<div class="file-drop-overlay" aria-live="polite"><span>＋</span><strong>Drop files to add them to your library</strong><small>PDF · DOCX · text files · images</small></div>{/if}
</div>
