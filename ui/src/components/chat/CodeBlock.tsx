// Fenced code block in answers: language label, Copy button, syntax highlighting (shiki, loaded on
// demand), horizontal scroll (never wraps). While an answer streams, a block whose closing fence has
// not arrived yet renders as plain text; it is highlighted once, when the fence closes, so streaming
// tokens never trigger a re-highlight or a flash.
import { memo, useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { copyText } from "@/hooks/dom";
import { cn } from "@/lib/utils";

const LANG_NAMES: Record<string, string> = {
  js: "JavaScript",
  javascript: "JavaScript",
  mjs: "JavaScript",
  cjs: "JavaScript",
  jsx: "JSX",
  ts: "TypeScript",
  typescript: "TypeScript",
  tsx: "TSX",
  py: "Python",
  python: "Python",
  rs: "Rust",
  rust: "Rust",
  ps1: "PowerShell",
  powershell: "PowerShell",
  pwsh: "PowerShell",
  sh: "Shell",
  bash: "Bash",
  shell: "Shell",
  shellscript: "Shell",
  zsh: "Zsh",
  console: "Console",
  bat: "Batch",
  cmd: "Batch",
  json: "JSON",
  jsonc: "JSON",
  html: "HTML",
  css: "CSS",
  md: "Markdown",
  markdown: "Markdown",
  yaml: "YAML",
  yml: "YAML",
  toml: "TOML",
  sql: "SQL",
  c: "C",
  cpp: "C++",
  "c++": "C++",
  cs: "C#",
  csharp: "C#",
  "c#": "C#",
  go: "Go",
  java: "Java",
  diff: "Diff",
  xml: "XML",
  ini: "INI",
  text: "Text",
  txt: "Text",
  plaintext: "Text",
};

/** Human name of a fence language ("ts" -> "TypeScript"); "Code" when there is none. */
export function languageName(lang: string): string {
  const l = lang.trim().toLowerCase();
  if (!l) return "Code";
  return LANG_NAMES[l] ?? lang.trim();
}

/** True when a markdown fence source (```lang ... ```) has its closing fence. */
export function fenceClosed(source: string): boolean {
  const lines = source.replace(/\s+$/, "").split(/\r?\n/);
  const open = /^\s{0,3}(`{3,}|~{3,})/.exec(lines[0] ?? "");
  if (!open) return true; // indented code: complete as soon as it is parsed
  if (lines.length < 2) return false;
  const fence = open[1]!;
  const last = lines[lines.length - 1]!.trim();
  return last.length >= fence.length && last === fence[0]!.repeat(last.length);
}

type Props = {
  code: string;
  lang: string;
  /** False while the closing fence is still streaming in: show plain text, don't highlight yet. */
  complete?: boolean;
};

export const CodeBlock = memo(function CodeBlock({ code, lang, complete = true }: Props) {
  const [html, setHtml] = useState<{ code: string; lang: string; html: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!complete) return;
    let live = true;
    import("@/lib/highlight")
      .then((m) => m.highlight(code, lang))
      .then((h) => {
        if (live && h) setHtml({ code, lang, html: h });
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [code, lang, complete]);

  useEffect(() => () => void (timer.current && clearTimeout(timer.current)), []);

  const onCopy = async () => {
    if (await copyText(code)) {
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1600);
    }
  };

  const name = languageName(lang);
  // Only show highlighted HTML that matches the current code, never a stale version.
  const highlighted = html && html.code === code && html.lang === lang ? html.html : null;

  return (
    <figure className="codeblock" data-testid="code-block" data-lang={lang || "text"} data-highlighted={highlighted ? "true" : "false"} aria-label={`${name} code`}>
      <div className="codeblock-bar">
        <span className="codeblock-lang">{name}</span>
        <button type="button" onClick={() => void onCopy()} className="codeblock-copy" aria-label={copied ? "Copied to the clipboard" : `Copy ${name} code`}>
          {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
          <span aria-hidden="true">{copied ? "Copied" : "Copy"}</span>
        </button>
        <span className="sr-only" aria-live="polite">
          {copied ? "Copied" : ""}
        </span>
      </div>
      {highlighted ? (
        <div className="codeblock-body" dangerouslySetInnerHTML={{ __html: highlighted }} />
      ) : (
        <div className="codeblock-body">
          <pre tabIndex={0} className={cn(!complete && "is-streaming")}>
            <code>{code}</code>
          </pre>
        </div>
      )}
    </figure>
  );
});
