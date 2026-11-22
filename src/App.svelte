<script lang="ts">
  import { onMount } from "svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import ContextView from "./lib/components/ContextView.svelte";
  import ExperimentView from "./lib/components/ExperimentView.svelte";
  import { getOllamaStatus } from "./lib/services/ollama";
  import type { ConnectionState, OllamaModel, OllamaStatus } from "./lib/types/ollama";

  let baseUrl = $state("http://localhost:11434");
  let connection = $state<ConnectionState>("checking");
  let status = $state<OllamaStatus | null>(null);
  let errorMessage = $state("");
  let isRefreshing = $state(false);
  let lastChecked = $state<Date | null>(null);
  let currentPage = $state<"dashboard" | "playground" | "context">("dashboard");
  let playgroundTab = $state<"chat" | "experiments">("chat");

  const runningNames = $derived(new Set(status?.runningModels ?? []));

  onMount(() => {
    void refreshStatus();
  });

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
      <button class="nav-item" disabled title="Analytics is coming later">
        <span class="nav-icon">▥</span><span class="nav-label">Analytics</span>
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
        <h1>{currentPage === "dashboard" ? "Dashboard" : currentPage === "playground" ? "Playground" : "Context workspace"}</h1>
      </div>
      <div class="topbar-meta">
        <span class="local-chip"><span class="privacy-dot"></span> Running locally</span>
        <div class="avatar" aria-label="Local workspace">H</div>
      </div>
    </header>

    {#if currentPage === "dashboard"}
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

    <section class="lower-grid">
      <article class="glass-card next-card">
        <div class="next-icon">✦</div>
        <div>
          <p class="eyebrow accent-text">UP NEXT</p>
          <h3>Controlled prompting experiments</h3>
          <p class="muted">Next, compare basic prompting, prompt engineering, and context engineering on the same question and model, with retrieved document passages visible.</p>
        </div>
        <span class="coming-soon">NEXT</span>
      </article>
      <article class="privacy-card">
        <span class="privacy-shield">⬡</span>
        <div><strong>Local model conversations</strong><p>Chat messages are sent to the configured Ollama address. This checkpoint does not connect to a cloud AI provider.</p></div>
      </article>
    </section>
    <footer class="page-footer"><span>ContextCraft</span><span>Local model dashboard · Checkpoint 01</span></footer>
    {:else if currentPage === "playground"}
      <div class="workspace-tabs" role="tablist" aria-label="Playground workspaces">
        <button class:active={playgroundTab === "chat"} role="tab" aria-selected={playgroundTab === "chat"} onclick={() => (playgroundTab = "chat")}>Local chat</button>
        <button class:active={playgroundTab === "experiments"} role="tab" aria-selected={playgroundTab === "experiments"} onclick={() => (playgroundTab = "experiments")}>Comparison lab</button>
      </div>
      {#if playgroundTab === "chat"}
        <ChatView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} />
      {:else}
        <ExperimentView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} />
      {/if}
    {:else}
      <ContextView models={status?.models ?? []} baseUrl={baseUrl} connected={connection === "connected"} />
    {/if}
  </main>
</div>
