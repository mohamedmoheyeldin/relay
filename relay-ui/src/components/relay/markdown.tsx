// Claude's replies are Markdown: bold, lists, links, code and tables.
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { api } from "@/lib/api";

const components: Components = {
    p: ({ children }) => <p className="my-2 first:mt-0 last:mb-0">{children}</p>,
    ul: ({ children }) => <ul className="my-2 list-disc space-y-1 pl-5">{children}</ul>,
    ol: ({ children }) => <ol className="my-2 list-decimal space-y-1 pl-5">{children}</ol>,
    strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
    h1: ({ children }) => <h3 className="mt-3 mb-1 text-md font-semibold">{children}</h3>,
    h2: ({ children }) => <h3 className="mt-3 mb-1 text-md font-semibold">{children}</h3>,
    h3: ({ children }) => <h4 className="mt-3 mb-1 text-sm font-semibold">{children}</h4>,
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
            <code className="rounded bg-secondary px-1 py-0.5 font-mono text-sm">{children}</code>
        ),
    pre: ({ children }) => <pre className="my-2 overflow-auto rounded-lg bg-secondary p-3 font-mono text-xs">{children}</pre>,
    table: ({ children }) => (
        <div className="my-2 overflow-auto">
            <table className="w-full border-collapse text-sm">{children}</table>
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
