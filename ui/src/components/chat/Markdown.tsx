// Markdown renderer (loaded on demand; react-markdown + GFM). Code blocks live in ./CodeBlock and
// highlight lazily with shiki once their fence has closed.
import { createContext, memo, useContext, type ComponentProps, type ReactElement } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { copyText } from "@/hooks/dom";
import { api, isTauri } from "@/lib/backend";
import { useApp } from "@/store/app";
import { CodeBlock, fenceClosed } from "./CodeBlock";

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

/** The markdown source and whether it is still streaming (to tell open fences from closed ones). */
const Source = createContext<{ text: string; streaming: boolean }>({ text: "", streaming: false });

function Pre({ children, node }: ComponentProps<"pre"> & { node?: { position?: { start: { offset?: number }; end: { offset?: number } } } }) {
  const src = useContext(Source);
  const child = (Array.isArray(children) ? children[0] : children) as ReactElement<ComponentProps<"code">> | undefined;
  const cls = child?.props?.className ?? "";
  const lang = /language-([\w#+.-]+)/.exec(cls)?.[1] ?? "";
  const raw = child?.props?.children;
  const code = (Array.isArray(raw) ? raw.join("") : String(raw ?? "")).replace(/\n$/, "");
  let complete = true;
  if (src.streaming) {
    const start = node?.position?.start.offset;
    const end = node?.position?.end.offset;
    complete = start != null && end != null ? fenceClosed(src.text.slice(start, end)) : false;
  }
  return <CodeBlock code={code} lang={lang} complete={complete} />;
}

const components: Components = {
  pre: Pre as Components["pre"],
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

export const Markdown = memo(function Markdown({ text, streaming = false }: { text: string; streaming?: boolean }) {
  return (
    <Source.Provider value={{ text, streaming }}>
      <div className="md" data-md="rendered">
        <ReactMarkdown remarkPlugins={plugins} components={components}>
          {text}
        </ReactMarkdown>
      </div>
    </Source.Provider>
  );
});
