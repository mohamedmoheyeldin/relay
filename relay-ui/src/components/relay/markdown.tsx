// Claude's replies are Markdown: bold, lists, links, code and tables.
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "@/lib/api";

const components: Components = {
    p: ({ children }) => <p className="my-3 first:mt-0 last:mb-0">{children}</p>,
    ul: ({ children }) => <ul className="my-3 list-disc space-y-1.5 pl-6 marker:text-fg-quaternary">{children}</ul>,
    ol: ({ children }) => <ol className="my-3 list-decimal space-y-1.5 pl-6 marker:text-fg-quaternary">{children}</ol>,
    strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
    h1: ({ children }) => <h3 className="mt-5 mb-2 text-lg font-semibold">{children}</h3>,
    h2: ({ children }) => <h3 className="mt-5 mb-2 text-lg font-semibold">{children}</h3>,
    h3: ({ children }) => <h4 className="mt-4 mb-1.5 text-md font-semibold">{children}</h4>,
    a: ({ href, children }) => (
        <a
            href={href}
            className="font-medium text-brand-secondary underline underline-offset-2 hover:text-brand-secondary_hover"
            onClick={(e) => {
                e.preventDefault();
                if (href) api.openPath(href);
            }}
        >
            {children}
        </a>
    ),
    code: ({ children, className }) =>
        className ? (
            <code className={className}>{children}</code>
        ) : (
            <code className="rounded-md bg-tertiary px-1.5 py-0.5 font-mono text-[0.85em]">{children}</code>
        ),
    pre: ({ children }) => <pre className="my-3 overflow-auto rounded-xl bg-secondary p-4 font-mono text-xs leading-relaxed ring-1 ring-secondary">{children}</pre>,
    table: ({ children }) => (
        <div className="my-3 overflow-auto">
            <table className="w-full border-collapse font-sans text-sm">{children}</table>
        </div>
    ),
    th: ({ children }) => <th className="border-b border-secondary px-2 py-1 text-left font-semibold">{children}</th>,
    td: ({ children }) => <td className="border-b border-secondary px-2 py-1">{children}</td>,
};

export const Markdown = ({ text }: { text: string }) => (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {text}
    </ReactMarkdown>
);
