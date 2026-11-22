<script lang="ts">
  import DOMPurify from "dompurify";
  import { marked } from "marked";

  let { source }: { source: string } = $props();

  const html = $derived(
    DOMPurify.sanitize(marked.parse(source, { gfm: true }) as string, {
      USE_PROFILES: { html: true },
    }),
  );
</script>

<div class="markdown-body">{@html html}</div>
