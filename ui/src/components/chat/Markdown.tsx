// Markdown renderer (loaded on demand; react-markdown + GFM). Code blocks highlight lazily with shiki.
import { memo, useEffect, useState, type ComponentProps, type ReactElement } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";
import { copyText } from "@/hooks/dom";
import { api, isTauri } from "@/lib/backend";
import { useApp } from "@/store/app";

async function openExternal(url: string) {
  if (!/^(https?:|mailto:)/i.test(url)) return;
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  try {
    await api.openUrl(url);
  } catch {
    const ok = await copyText(url);
    useApp.getState().toast(ok ? "Link copied to the clipboard" : url);
  }
}

function CodeBlock({ code, lang }: { code: string; lang: string }) {
  const [html, setHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // Highlight lazily and debounce while the block is still streaming in.
  useEffect(() => {
    let live = true;
    const t = setTimeout(
      () => {
        import("@/lib/highlight")
          .then((m) => m.highlight(code, lang))
          .then((h) => live && setHtml(h))
          .catch(() => undefined);
      },
      html ? 120 : 0,
    );
    return () => {
      live = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-highlight only when the code changes
  }, [code, lang]);

  return (
    <div className="codeblock">
      <div className="flex h-8 items-center justify-between border-b border-line px-3 text-caption text-fg-muted select-none">
        <span className="font-mono text-[11.5px]">{lang || "text"}</span>
        <button
          type="button"
          onClick={async () => {
            if (await copyText(code)) {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            }
          }}
          className="inline-flex items-center gap-1.5 rounded-sm px-1.5 py-0.5 transition-colors hover:bg-hover hover:text-fg"
          aria-label="Copy code"
        >
          {copied ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {html ? (
        <div dangerouslySetInnerHTML={{ __html: html }} />
      ) : (
        <pre>
          <code>{code}</code>
        </pre>
      )}
    </div>
  );
}

const components: Components = {
  pre({ children }) {
    const child = (Array.isArray(children) ? children[0] : children) as ReactElement<ComponentProps<"code">> | undefined;
    const cls = child?.props?.className ?? "";
    const lang = /language-([\w#+.-]+)/.exec(cls)?.[1] ?? "";
    const raw = child?.props?.children;
    const code = (Array.isArray(raw) ? raw.join("") : String(raw ?? "")).replace(/\n$/, "");
    return <CodeBlock code={code} lang={lang} />;
  },
  table({ children }) {
    return (
      <div className="table-wrap scroll-thin">
        <table>{children}</table>
      </div>
    );
  },
  a({ href, children }) {
    return (
      <a
        href={href}
        onClick={(e) => {
          e.preventDefault();
          if (href) void openExternal(href);
        }}
        title={href}
      >
        {children}
      </a>
    );
  },
  img({ alt }) {
    return <span className="text-fg-muted">[{alt || "image"}]</span>;
  },
};

const plugins = [remarkGfm];

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md" data-md="rendered">
      <ReactMarkdown remarkPlugins={plugins} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
