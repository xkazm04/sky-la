// @vitest-environment happy-dom
import type { AdvisorStatusDto } from "@skyla/ipc";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AdvisorConnection, connectionBadge } from "./AdvisorConnection";

const base: AdvisorStatusDto = {
  provider: "claude-code-cli",
  state: "ready",
  version: "2.1.293 (Claude Code)",
  lookedIn: [],
  resetsAt: null,
  demo: false,
};

afterEach(cleanup);

describe("AdvisorConnection", () => {
  it("says it's ready and never asks for credentials", () => {
    render(<AdvisorConnection status={base} />);
    expect(screen.getByText("Ready")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Advisor connection" }).textContent).toContain(
      "never sees your Claude credentials",
    );
  });

  it("explains how to install when the CLI isn't found, with where it looked", () => {
    render(
      <AdvisorConnection
        status={{ ...base, state: "not_installed", version: null, lookedIn: ["/usr/bin/claude"] }}
      />,
    );
    expect(screen.getByText("Claude Code not found")).toBeTruthy();
    expect(screen.getByText("claude auth login")).toBeTruthy();
    expect(screen.getByText("/usr/bin/claude")).toBeTruthy();
  });

  it("sends the user to their terminal to sign in, and can check again", () => {
    const again = vi.fn();
    render(<AdvisorConnection status={{ ...base, state: "not_signed_in" }} onCheckAgain={again} />);
    expect(screen.getByText("Not signed in")).toBeTruthy();
    expect(screen.getByRole("region").textContent).toContain("never asks for or stores");
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(again).toHaveBeenCalledOnce();
  });

  it("says when a usage limit resets and that the books still work", () => {
    render(
      <AdvisorConnection
        status={{ ...base, state: "rate_limited", resetsAt: "2026-10-12T12:00:00Z" }}
      />,
    );
    const text = screen.getByRole("region").textContent ?? "";
    expect(text).toContain("resets on 12 Oct 2026, 12:00 UTC");
    expect(text).toContain("the books work as usual");
  });

  it("labels the demo as recorded runs", () => {
    expect(connectionBadge({ ...base, provider: "fake", demo: true }).label).toBe(
      "Demo · recorded runs",
    );
  });
});
