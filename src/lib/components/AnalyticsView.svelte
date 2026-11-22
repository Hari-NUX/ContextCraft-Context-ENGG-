<script lang="ts">
  import { onMount } from "svelte";
  import { clearExperimentMetrics, deleteExperimentGroup, listChatUsageMetrics, listExperimentMetrics } from "../services/ollama";
  import type { ChatUsageEntry, ExperimentMetricEntry } from "../types/analytics";

  type ExperimentGroup = { id: string; createdAt: number; model: string; documentName: string; question: string; entries: ExperimentMetricEntry[] };
  type ChatGroup = { id: string; title: string; model: string; updatedAt: number; replies: number; inputTokens: number; outputTokens: number };

  let entries = $state<ExperimentMetricEntry[]>([]);
  let chatUsage = $state<ChatUsageEntry[]>([]);
  let isLoading = $state(true);
  let errorMessage = $state("");
  let isMutating = $state(false);

  const groups = $derived.by(() => {
    const grouped = new Map<string, ExperimentMetricEntry[]>();
    for (const entry of entries) {
      const group = grouped.get(entry.experimentId) ?? [];
      group.push(entry);
      grouped.set(entry.experimentId, group);
    }
    return [...grouped.entries()]
      .map(([id, metrics]): ExperimentGroup => ({
        id,
        entries: metrics.sort((left, right) => {
          const order = ["Basic Prompting", "Prompt Engineering", "Full Document Context", "Light Context", "Standard Context", "Context Engineering"];
          return order.indexOf(left.label) - order.indexOf(right.label);
        }),
        createdAt: Math.max(...metrics.map((entry) => entry.createdAt)),
        model: metrics[0]?.model ?? "Unknown model",
        documentName: metrics[0]?.documentName ?? "Unknown document",
        question: metrics[0]?.question ?? "",
      }))
      .sort((left, right) => right.createdAt - left.createdAt);
  });
  const totalTokens = $derived(entries.reduce((sum, entry) => sum + (entry.inputTokens ?? 0) + (entry.outputTokens ?? 0), 0));
  const chatGroups = $derived.by(() => {
    const grouped = new Map<string, ChatGroup>();
    for (const entry of chatUsage) {
      const group = grouped.get(entry.sessionId) ?? {
        id: entry.sessionId,
        title: entry.title,
        model: entry.model,
        updatedAt: entry.updatedAt,
        replies: 0,
        inputTokens: 0,
        outputTokens: 0,
      };
      group.replies += 1;
      group.inputTokens += entry.inputTokens ?? 0;
      group.outputTokens += entry.outputTokens ?? 0;
      if (entry.updatedAt >= group.updatedAt) {
        group.title = entry.title;
        group.model = entry.model;
        group.updatedAt = entry.updatedAt;
      }
      grouped.set(entry.sessionId, group);
    }
    return [...grouped.values()].sort((left, right) => right.updatedAt - left.updatedAt);
  });
  const allTokenCount = $derived(totalTokens + chatUsage.reduce((sum, entry) => sum + (entry.inputTokens ?? 0) + (entry.outputTokens ?? 0), 0));
  const successfulRuns = $derived(entries.filter((entry) => entry.status === "done"));
  const averageSpeed = $derived.by(() => {
    const speeds = successfulRuns
      .filter((entry) => entry.outputTokens !== undefined && entry.evalDurationNs && entry.evalDurationNs > 0)
      .map((entry) => (entry.outputTokens! / entry.evalDurationNs!) * 1_000_000_000);
    return speeds.length ? speeds.reduce((sum, speed) => sum + speed, 0) / speeds.length : 0;
  });

  onMount(() => {
    void refresh();
    window.addEventListener("contextcraft:analytics-updated", refresh);
    return () => window.removeEventListener("contextcraft:analytics-updated", refresh);
  });

  async function refresh() {
    isLoading = true;
    errorMessage = "";
    try {
      const [comparisonEntries, conversationEntries] = await Promise.all([
        listExperimentMetrics(600),
        listChatUsageMetrics(),
      ]);
      entries = comparisonEntries;
      chatUsage = conversationEntries;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      isLoading = false;
    }
  }

  async function removeGroup(group: ExperimentGroup) {
    if (!window.confirm(`Delete the saved metrics for this comparison?`)) return;
    isMutating = true;
    try {
      await deleteExperimentGroup(group.id);
      entries = entries.filter((entry) => entry.experimentId !== group.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      isMutating = false;
    }
  }

  async function clearHistory() {
    if (!entries.length || !window.confirm("Clear all locally saved comparison metrics? Saved chat history will remain.")) return;
    isMutating = true;
    try {
      await clearExperimentMetrics();
      entries = [];
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      isMutating = false;
    }
  }

  function durationLabel(entry: ExperimentMetricEntry) {
    const nanoseconds = entry.totalDurationNs ?? entry.clientDurationNs;
    return typeof nanoseconds === "number" ? `${(nanoseconds / 1_000_000_000).toFixed(2)} s` : "—";
  }

  function speedLabel(entry: ExperimentMetricEntry) {
    if (!entry.outputTokens || !entry.evalDurationNs) return "—";
    return `${((entry.outputTokens / entry.evalDurationNs) * 1_000_000_000).toFixed(1)} tok/s`;
  }

  function tokenLabel(value: number | undefined) {
    return typeof value === "number" ? value.toLocaleString() : "—";
  }

  function dateLabel(value: number) {
    return new Date(value).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
  }
</script>

<section class="analytics-page">
  <div class="analytics-heading">
    <div><p class="eyebrow accent-text">CHECKPOINT 08</p><h2>Analytics lab</h2><p class="muted">Review token usage from saved chats and compare the three prompting approaches.</p></div>
    <div class="analytics-actions"><button class="secondary-button" onclick={refresh} disabled={isLoading}>⟳ Refresh</button><button class="clear-chat-button" onclick={clearHistory} disabled={isMutating || entries.length === 0}>Clear comparison history</button></div>
  </div>

  {#if errorMessage}<div class="storage-error" role="alert">{errorMessage}</div>{/if}

  <div class="analytics-summary-grid">
    <article class="glass-card analytics-summary"><span class="analytics-summary-icon">▦</span><div><p class="eyebrow">COMPARISONS</p><strong>{groups.length}</strong><small>{entries.length} individual runs</small></div></article>
    <article class="glass-card analytics-summary"><span class="analytics-summary-icon">▤</span><div><p class="eyebrow">SAVED CHATS</p><strong>{chatGroups.length}</strong><small>{chatUsage.length} assistant replies</small></div></article>
    <article class="glass-card analytics-summary"><span class="analytics-summary-icon">◈</span><div><p class="eyebrow">TOKENS PROCESSED</p><strong>{allTokenCount.toLocaleString()}</strong><small>Saved chats and comparison runs</small></div></article>
    <article class="glass-card analytics-summary"><span class="analytics-summary-icon">ϟ</span><div><p class="eyebrow">AVG COMPARISON SPEED</p><strong>{averageSpeed ? `${averageSpeed.toFixed(1)}` : "—"}</strong><small>Output tokens per second</small></div></article>
  </div>

  <div class="analytics-history-heading"><div><p class="eyebrow">CHAT USAGE</p><h3>Saved conversations</h3></div><span>{chatGroups.length} chats</span></div>
  {#if isLoading}
    <div class="analytics-empty glass-card"><span class="spin">⟳</span><p>Loading saved chat usage…</p></div>
  {:else if chatGroups.length === 0}
    <div class="analytics-empty glass-card"><strong>No saved assistant replies with usage yet</strong><p>Completed replies from your saved Playground chats will appear here.</p></div>
  {:else}
    <div class="analytics-table-wrap analytics-chat-table-wrap"><table class="analytics-table"><thead><tr><th>Chat</th><th>Model</th><th>Updated</th><th>Replies</th><th>Input tokens</th><th>Output tokens</th></tr></thead>
      <tbody>{#each chatGroups as chat (chat.id)}<tr><td><strong>{chat.title}</strong></td><td>{chat.model}</td><td>{dateLabel(chat.updatedAt)}</td><td>{chat.replies}</td><td>{tokenLabel(chat.inputTokens)}</td><td>{tokenLabel(chat.outputTokens)}</td></tr>{/each}</tbody>
    </table></div>
  {/if}

  <div class="analytics-history-heading"><div><p class="eyebrow">LOCAL HISTORY</p><h3>Comparison runs</h3></div><span>{groups.length} saved</span></div>
  {#if isLoading}
    <div class="analytics-empty glass-card"><span class="spin">⟳</span><p>Loading local comparison metrics…</p></div>
  {:else if groups.length === 0}
    <div class="analytics-empty glass-card"><span class="empty-icon">▥</span><strong>No comparison metrics yet</strong><p>Run a three-mode comparison from Playground. Completed run metrics will be stored locally here.</p></div>
  {:else}
    <div class="analytics-groups">
      {#each groups as group (group.id)}
        <article class="analytics-group glass-card">
          <div class="analytics-group-header">
            <div><p class="eyebrow">{dateLabel(group.createdAt)} <span>·</span> {group.model} <span>·</span> {group.documentName}</p><h3>{group.question}</h3></div>
            <button class="document-delete" onclick={() => removeGroup(group)} disabled={isMutating} aria-label="Delete comparison metrics" title="Delete comparison">×</button>
          </div>
          <div class="analytics-table-wrap">
            <table class="analytics-table"><thead><tr><th>Approach</th><th>Status</th><th>Input</th><th>Output</th><th>Time</th><th>Speed</th><th>Context chunks</th></tr></thead>
              <tbody>{#each group.entries as entry (entry.id)}
                <tr><td><strong>{entry.label}</strong><small>{entry.mode}</small></td><td><span class="analytics-run-status" class:success={entry.status === "done"}>{entry.status}</span></td><td>{tokenLabel(entry.inputTokens)}</td><td>{tokenLabel(entry.outputTokens)}</td><td>{durationLabel(entry)}</td><td>{speedLabel(entry)}</td><td>{entry.retrievedChunks}</td></tr>
              {/each}</tbody>
            </table>
          </div>
          <p class="analytics-footnote">Timings and token counts are reported by Ollama when available. This history stores metrics and question metadata, not generated answer text.</p>
        </article>
      {/each}
    </div>
  {/if}

  <div class="chat-footnote"><span class="privacy-dot"></span><span>Chat usage is read from saved chat files; comparison metrics are stored in your local SQLite file. Clearing comparison history leaves chats intact.</span></div>
</section>
