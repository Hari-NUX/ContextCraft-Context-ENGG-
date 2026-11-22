# ContextCraft

A desktop learning studio for comparing Basic Prompting, Prompt Engineering, and Context Engineering with local and optional cloud AI models.

## Planned stack

- Tauri 2 desktop shell
- Svelte 5, TypeScript, and Vite interface
- pnpm as the project package manager
- Python and FastAPI document/context service
- Ollama for local models; optional user-configured provider APIs
- SQLite for local chat and project metadata

## Planned workspaces

- Dashboard and model status
- Prompting experiment playground
- PDF context workspace with visible retrieved passages
- Analytics and saved conversations
- Prompt generator with copy-ready prompts and document-scope guidance
- Optional, user-controlled web retrieval

## First implementation checkpoint

The first checkpoint is the desktop shell with an editable Ollama endpoint, service status, installed model discovery, running model discovery, and a manual refresh action. The second checkpoint adds streaming chat, model selection, and sanitized Markdown rendering for headings, code, and tables. The third checkpoint adds local chat sessions, automatic titles, session rename/delete, and a small context snapshot per chat. The fourth checkpoint adds local semantic history retrieval using Ollama embeddings and SQLite. The fifth checkpoint adds local PDF import, page-text extraction, and chunk inspection. The sixth checkpoint embeds PDF chunks locally, retrieves relevant passages for a question, and displays source page references with a grounded answer. The seventh checkpoint adds a sequential comparison lab for Basic Prompting, Prompt Engineering, and Context Engineering using the same model and question. The eighth checkpoint provides an Analytics page with saved chat usage and comparison metrics, including token counts, timings, generation speed, and run history. Checkpoint nine adds a local prompt generator that creates reusable prompts and gives page-scope advice for saved PDFs. Later checkpoints will add provider APIs and optional web retrieval.

The prompt generator is a deterministic local form; it does not call an LLM. It uses a saved file's name and page count only to suggest whether to share the complete file, relevant chapter pages, page images, or a focused excerpt. It does not upload the original source file or save the prompt inputs.

The shared source library is reachable from the top bar on every route. Current import support is selectable-text PDFs, TXT, and Markdown. Extracted text is shared with the Context workspace, optional chat source picker, comparison lab, and prompt generator. New imports are indexed automatically when Ollama is connected; users can run or repeat indexing manually in Context. The comparison selector lists indexed and unindexed files, supports a no-file choice (which runs the first two approaches and marks the context approach skipped), and has an add-file action. The comparison context mode can be Light, Standard, or Full file; Full file is capped at 6,000 extracted characters to stay within the configured local context window. Basic Prompting and Prompt Engineering intentionally remain no-source baselines, so token savings should be evaluated between Full file and retrieval modes. Saved comparison metrics trigger an Analytics refresh when a run completes. Original source files and images are not retained; image understanding and additional document formats require a later multimodal/import checkpoint. Long chat and comparison responses remain in their own scroll panels, and active streams continue while switching routes because those workspaces stay mounted.

Chat data is stored under Tauri's per-user application data folder. Each session has a `context.json` file for its title, model, timestamps, and short context snapshot, plus a `messages.json` file for the full conversation.

On Windows, the configured application identifier is `dev.contextcraft.desktop`, so local app data is stored under `%LOCALAPPDATA%\dev.contextcraft.desktop`. The semantic vector index is `chat-memory.sqlite` there; it is an ordinary SQLite database containing text and embedding vectors as binary blobs, ranked in Rust with cosine similarity. This is a local file, not a hosted vector database. Chat session files are under `chats\<chat-id>`. The PDF checkpoint stores extracted text as `documents\<document-id>\document.json`; it does not retain the original PDF file.

For semantic history search, install the embedding model once from PowerShell with `ollama pull nomic-embed-text`. In the Playground, select **Index saved chats** to build a local SQLite index. The app does not download this model automatically. If it's unavailable, ordinary chat continues without cross-chat recall. The embedding-only model is excluded from the chat model selector.

In Context, select **Index PDF chunks** to create embeddings for the visible chunk set. ContextCraft uses Ollama's batch embedding endpoint when available and falls back to its legacy single-text endpoint for older Ollama versions. For a document question, ContextCraft embeds the question, ranks that document's chunks using cosine similarity, and sends the top relevant excerpt text (with page labels) to the selected local chat model. Explicit requests for the first N pages select those pages directly. Chat requests set Ollama's `num_ctx` to 4,096 to avoid very small context settings in some local model manifests. The numeric vectors are for search; the language model receives the original text excerpts. Chat and document vectors use separate SQLite tables in the same `chat-memory.sqlite` file.

API credentials and local conversation/document data must stay out of Git. The application will store those locally and explain when content is sent to a selected external provider.

The Tauri backend is Rust (`src-tauri/src/lib.rs`, launched by `src-tauri/src/main.rs`). The user interface is Svelte and TypeScript (`src/App.svelte` and `src/lib/components`). `src-tauri/tauri.conf.json` is JSON configuration for the window, bundle identifier, and build commands; it is not the application logic.

The PDF UI and retrieval flow are in `src/lib/components/ContextView.svelte`. It uses PDF.js to extract page text locally and sends chunk/query embedding requests through Rust in `src-tauri/src/lib.rs`. The comparison lab is `src/lib/components/ExperimentView.svelte`; saved metrics appear in `src/lib/components/AnalyticsView.svelte`. Install new frontend dependencies with pnpm after pulling changes:

```powershell
pnpm install
```

## Development commands

Use pnpm for JavaScript dependencies and scripts:

```powershell
pnpm install
pnpm exec tauri dev
```

chat memry is saved here

C:\Users\harik\AppData\Local\dev.contextcraft.desktop\chat-memory.sqlite
