// @vitest-environment jsdom
// axe-core on the main views (structure, names, roles). Colour contrast runs in Playwright.
import { beforeEach, describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { renderUi, useMockBackend, axeViolations } from "@/test/render";
import { api } from "@/lib/backend";
import { Home } from "@/components/chat/Home";
import { ChatView } from "@/components/chat/ChatView";
import TasksPage from "@/pages/TasksPage";
import SettingsPage from "@/settings/SettingsPage";
import Installer from "@/pages/Installer";

describe("main views have no serious axe violations", () => {
  beforeEach(() => {
    useMockBackend();
  });

  it("home", async () => {
    const { container } = renderUi(<Home />);
    await screen.findByRole("radio", { name: /Hybrid/ });
    expect(await axeViolations(container)).toEqual([]);
  });

  it("chat with a task timeline", async () => {
    const convs = await api.convList();
    const seeded = convs.find((c) => c.title === "Tidy up my Downloads folder")!;
    const { container } = renderUi(<ChatView convId={seeded.id} />);
    await screen.findByTestId("task-timeline");
    expect(await axeViolations(container)).toEqual([]);
  });

  it("task history with a selected task", async () => {
    const { container } = renderUi(
      <Routes>
        <Route path="/tasks/:id?" element={<TasksPage />} />
      </Routes>,
      { route: "/tasks/wf-2" },
    );
    await screen.findByTestId("task-list");
    await screen.findByTestId("task-timeline");
    expect(await axeViolations(container)).toEqual([]);
  });

  it("settings (general and appearance)", async () => {
    for (const section of ["general", "appearance", "local-ai", "keyboard"]) {
      const { container, unmount } = renderUi(
        <Routes>
          <Route path="/settings/:section?" element={<SettingsPage />} />
        </Routes>,
        { route: `/settings/${section}` },
      );
      await screen.findByRole("main");
      await new Promise((r) => setTimeout(r, 250));
      expect(await axeViolations(container), section).toEqual([]);
      unmount();
    }
  });

  it("installer", async () => {
    const { container } = renderUi(
      <Installer info={{ mode: "install", version: "0.3.0", install_dir: "C:\\Program Files\\Relay", installed_version: null, data_dir: "C:\\x", data_bytes: 1e9 }} />,
    );
    expect(await axeViolations(container)).toEqual([]);
  });
});
