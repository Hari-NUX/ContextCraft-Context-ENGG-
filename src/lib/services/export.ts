import DOMPurify from "dompurify";
import { marked } from "marked";

export function exportMarkdownAsPdf(title: string, markdown: string) {
  const safeTitle = DOMPurify.sanitize(title);
  const safeBody = DOMPurify.sanitize(marked.parse(markdown, { gfm: true }) as string, { USE_PROFILES: { html: true } });
  const printDocument = `<!doctype html><html><head><meta charset="utf-8"><title>${safeTitle}</title><style>
    @page{margin:18mm}*{box-sizing:border-box}body{font:11pt/1.55 Arial,sans-serif;color:#20202a;max-width:900px;margin:0 auto}h1{font-size:22pt;border-bottom:2px solid #7451a6;padding-bottom:8px}h2{font-size:16pt;margin-top:24px}h3{font-size:13pt}p,li{overflow-wrap:anywhere}blockquote{border-left:3px solid #8064a9;padding:2px 14px;color:#4d4854}pre{white-space:pre-wrap;background:#f3f1f6;padding:12px;border-radius:6px}code{font:9pt Consolas,monospace}table{width:100%;border-collapse:collapse;margin:12px 0;break-inside:auto}thead{display:table-header-group}tr{break-inside:avoid}th,td{border:1px solid #bcb6c7;padding:6px 8px;text-align:left;vertical-align:top}th{background:#eeeaf4}img{max-width:100%}.meta{color:#777;font-size:9pt;border-bottom:1px solid #ddd;padding-bottom:10px}@media print{a{color:inherit;text-decoration:none}}
  </style></head><body><h1>${safeTitle}</h1><p class="meta">Exported from ContextCraft · ${new Date().toLocaleString()}</p>${safeBody}</body></html>`;
  const frame = document.createElement("iframe");
  frame.title = `Print preview: ${title}`;
  frame.setAttribute("aria-hidden", "true");
  Object.assign(frame.style, { position: "fixed", right: "0", bottom: "0", width: "1px", height: "1px", border: "0", opacity: "0", pointerEvents: "none" });
  frame.srcdoc = printDocument;
  const removeFrame = () => window.setTimeout(() => frame.remove(), 1000);
  frame.onload = () => {
    const printWindow = frame.contentWindow;
    if (!printWindow) { frame.remove(); throw new Error("Could not open the print dialog."); }
    printWindow.addEventListener("afterprint", removeFrame, { once: true });
    printWindow.focus();
    printWindow.print();
    window.setTimeout(() => frame.remove(), 60_000);
  };
  document.body.appendChild(frame);
}
