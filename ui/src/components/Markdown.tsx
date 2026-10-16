import { memo, useEffect, useState, type ComponentProps, type ReactElement } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";
import { copyText } from "./hooks";
import { isTauri, transport } from "../lib/backend";
import { useApp } from "../store/app";

async function openExternal(url: string) {
  if (!/^(https?:|mailto:)/i.test(url)) return;
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  try {
    await (await transport()).invoke("open_url", { url });
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
    const t = setTimeout(() => {
      import("../lib/highlight")
        .then((m) => m.highlight(code, lang))
        .then((h) => live && setHtml(h))
        .catch(() => undefined);
    }, html ? 120 : 0);
    return () => {
      live = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [code, lang]);

  return (
    <div className="codeblock">
      <div className="flex h-9 items-center justify-between bg-[var(--code-head)] px-4 text-[12.5px] text-muted select-none">
        <span>{lang || "text"}</span>
        <button
          type="button"
          onClick={async () => {
            if (await copyText(code)) {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            }
          }}
          className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 hover:text-fg"
          aria-label="Copy code"
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
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
    return <span className="text-subtle">[{alt || "image"}]</span>;
  },
};

const plugins = [remarkGfm];

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={plugins} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
