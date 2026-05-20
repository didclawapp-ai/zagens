import { describe, expect, it } from "vitest";
import {
  DEFAULT_MARKERS,
  DIDCLAW_PHEROMONE_MARKERS,
  injectMemorySection,
} from "../src/index.js";

describe("injectMemorySection", () => {
  it("appends section when markers absent", () => {
    const body = "# AGENTS\n\nHello.";
    const content = "## Memory\n\ndata";
    const out = injectMemorySection(body, content, DEFAULT_MARKERS);
    expect(out).toContain("Hello.");
    expect(out).toContain(DEFAULT_MARKERS.start);
    expect(out).toContain("## Memory");
    expect(out).toContain(DEFAULT_MARKERS.end);
  });

  it("replaces existing bounded section", () => {
    const inner = "old";
    const body = `intro\n${DEFAULT_MARKERS.start}\n${inner}\n${DEFAULT_MARKERS.end}\ntrailer`;
    const out = injectMemorySection(body, "new", DEFAULT_MARKERS);
    expect(out).toContain("new");
    expect(out).not.toContain("old");
    expect(out).toContain("intro");
    expect(out).toContain("trailer");
  });

  it("supports DidClaw marker aliases", () => {
    const body = `${DIDCLAW_PHEROMONE_MARKERS.start}\nold\n${DIDCLAW_PHEROMONE_MARKERS.end}`;
    const out = injectMemorySection(body, "fresh", DIDCLAW_PHEROMONE_MARKERS);
    expect(out).toContain("fresh");
    expect(out).not.toContain("old");
  });

  it("returns only section for empty file", () => {
    const out = injectMemorySection("", "x", DEFAULT_MARKERS);
    expect(out.trim()).toBe(`${DEFAULT_MARKERS.start}\nx\n${DEFAULT_MARKERS.end}`);
  });
});
