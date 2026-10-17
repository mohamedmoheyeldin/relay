// Component test helpers: providers, the mock backend with a scenario, and an axe check.
import type { ReactElement, ReactNode } from "react";
import { render } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import axe from "axe-core";
import { queryClient } from "@/lib/query";
import { setTransport, type Transport } from "@/lib/backend";
import { createMockTransport } from "@/lib/mock";
import { readScenario } from "@/lib/mock/scenario";
import { TooltipProvider } from "@/components/ui/tooltip";

export function useMockBackend(flags = "", flow?: string): Transport {
  const t = createMockTransport(readScenario(`?scenario=fast,${flags}${flow ? `&flow=${flow}` : ""}`));
  setTransport(t);
  queryClient.clear();
  return t;
}

export function renderUi(ui: ReactElement, { route = "/" }: { route?: string } = {}) {
  const Wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={0}>
        <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>
  );
  return render(ui, { wrapper: Wrapper });
}

/** Runs axe-core on a container and returns serious/critical violations (colour contrast is
 *  checked in the browser by Playwright; jsdom cannot compute it). */
export async function axeViolations(container: Element) {
  const res = await axe.run(container, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
    resultTypes: ["violations"],
  });
  return res.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(" ")).join(", ")})`);
}
