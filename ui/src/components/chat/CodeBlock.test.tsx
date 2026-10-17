// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axeViolations } from "@/test/render";

// Count highlight calls: streaming must not re-highlight on every token.
const { highlight } = vi.hoisted(() => ({
  highlight: vi.fn(async (code?: string, lang?: string) => `<pre class="shiki" tabindex="0"><code><span class="line">${lang ?? ""}:${code ?? ""}</span></code></pre>`),
}));
vi.mock("@/lib/highlight", () => ({ highlight: (code: string, lang: string) => highlight(code, lang) }));

const { CodeBlock, fenceClosed, languageName } = await import("./CodeBlock");
const { Markdown } = await import("./Markdown");

beforeEach(() => highlight.mockClear());

describe("fence and language helpers", () => {
  it("tells an open fence from a closed one", () => {
    expect(fenceClosed("```ts\nconst a = 1;")).toBe(false);
    expect(fenceClosed("```ts\nconst a = 1;\n``")).toBe(false);
    expect(fenceClosed("```ts\nconst a = 1;\n```")).toBe(true);
    expect(fenceClosed("````md\n```\n````\n")).toBe(true);
    expect(fenceClosed("~~~\nx\n~~~")).toBe(true);
    expect(fenceClosed("    indented code")).toBe(true);
  });
  it("names languages for the label", () => {
    expect(languageName("ts")).toBe("TypeScript");
    expect(languageName("PS1")).toBe("PowerShell");
    expect(languageName("")).toBe("Code");
    expect(languageName("elixir")).toBe("elixir");
  });
});

describe("CodeBlock", () => {
  it("has a language label, an accessible name and a keyboard-reachable Copy button that confirms", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<CodeBlock code={'console.log("hi")'} lang="js" />);
    const fig = screen.getByRole("figure", { name: "JavaScript code" });
    expect(fig).toHaveTextContent("JavaScript");
    const btn = screen.getByRole("button", { name: "Copy JavaScript code" });
    btn.focus();
    await userEvent.keyboard("{Enter}");
    expect(writeText).toHaveBeenCalledWith('console.log("hi")');
    expect(screen.getByRole("button", { name: "Copied to the clipboard" })).toHaveTextContent("Copied");
    await waitFor(() => expect(fig).toHaveAttribute("data-highlighted", "true"));
  });

  it("does not highlight an unfinished block, then highlights once when it is complete", async () => {
    const { rerender } = render(<CodeBlock code="const a" lang="ts" complete={false} />);
    rerender(<CodeBlock code="const a = 1" lang="ts" complete={false} />);
    rerender(<CodeBlock code="const a = 1;" lang="ts" complete={false} />);
    await act(async () => undefined);
    expect(highlight).not.toHaveBeenCalled();
    expect(screen.getByTestId("code-block")).toHaveAttribute("data-highlighted", "false");
    expect(screen.getByText("const a = 1;")).toBeInTheDocument(); // plain text shows meanwhile
    rerender(<CodeBlock code="const a = 1;" lang="ts" complete />);
    await waitFor(() => expect(screen.getByTestId("code-block")).toHaveAttribute("data-highlighted", "true"));
    expect(highlight).toHaveBeenCalledTimes(1);
  });

  it("has no serious accessibility violations", async () => {
    const { container } = render(<CodeBlock code={"a\nb"} lang="python" />);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("Markdown streaming", () => {
  it("keeps inline code inline and highlights a fenced block only after its closing fence arrives", async () => {
    const parts = ["Use `npx playwright test`.\n\n```ts\n", "import { test } ", "from \"@playwright/test\";\n", "test(\"a\", async () => {});\n", "```\n\nDone."];
    let text = "";
    const { rerender, container } = render(<Markdown text={text} streaming />);
    for (const p of parts.slice(0, -1)) {
      text += p;
      rerender(<Markdown text={text} streaming />);
    }
    await act(async () => undefined);
    expect(highlight).not.toHaveBeenCalled();
    expect(container.querySelector("p code")).toHaveTextContent("npx playwright test");
    text += parts.at(-1)!;
    rerender(<Markdown text={text} streaming />);
    rerender(<Markdown text={text} streaming={false} />);
    await waitFor(() => expect(screen.getByTestId("code-block")).toHaveAttribute("data-highlighted", "true"));
    expect(highlight).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("figure", { name: "TypeScript code" })).toBeInTheDocument();
  });
});
