// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

/** The `--sk-*` declarations of the first block that starts with `selector {`. */
function block(selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no block ${selector}`);
  const end = css.indexOf("\n}", start);
  const body = css.slice(start, end);
  const tokens = new Map<string, string>();
  for (const match of body.matchAll(/(--sk-[a-z0-9-]+):\s*([^;]+);/g)) {
    tokens.set(match[1] as string, (match[2] as string).replace(/\s+/g, " ").trim());
  }
  return tokens;
}

type Rgba = [number, number, number, number];

function parse(value: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex) {
    const n = Number.parseInt(hex[1] as string, 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgba = /^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)$/.exec(value);
  if (rgba) return [Number(rgba[1]), Number(rgba[2]), Number(rgba[3]), Number(rgba[4])];
  throw new Error(`can't parse colour ${value}`);
}

/** Composites `top` over an opaque `bottom`. */
function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top[3];
  return [0, 1, 2]
    .map((i) => (top[i] as number) * a + (bottom[i] as number) * (1 - a))
    .concat(1) as Rgba;
}

function luminance([r, g, b]: Rgba): number {
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrast(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const light = block(":root");
const dark = block(':root[data-appearance="dark"]');

describe("design tokens", () => {
  it("define every token in both appearances", () => {
    expect([...dark.keys()].sort()).toEqual([...light.keys()].sort());
    expect(light.size).toBeGreaterThanOrEqual(35);
  });

  it("map every token into the Tailwind theme", () => {
    for (const name of light.keys()) {
      if (name.includes("solid") || name === "--sk-capsule-solid") continue;
      expect(css, name).toContain(`var(${name})`);
    }
  });

  for (const [appearance, tokens] of [
    ["light", light],
    ["dark", dark],
  ] as const) {
    describe(`${appearance} appearance`, () => {
      const color = (name: string) => parse(tokens.get(name) ?? "missing");
      const backdrop = color("--sk-backdrop");
      const surface = color("--sk-surface");
      const panel = over(color("--sk-panel"), backdrop);
      const raised = color("--sk-surface-raised");
      const selected = over(color("--sk-accent-tint"), panel);
      // A key hint inside a glass capsule on the window backdrop.
      const keycap = over(color("--sk-fill"), over(color("--sk-capsule"), backdrop));

      it.each([
        ["--sk-ink", surface],
        ["--sk-ink", panel],
        ["--sk-ink-secondary", surface],
        ["--sk-ink-secondary", panel],
        ["--sk-ink-secondary", raised],
        ["--sk-ink-tertiary", surface],
        ["--sk-accent-ink", surface],
        ["--sk-accent-ink", selected],
        ["--sk-ink", selected],
        // Toolbar subtitles and the status line sit on the backdrop itself.
        ["--sk-ink-secondary", backdrop],
        ["--sk-ink-secondary", keycap],
      ])("%s is readable (AA) on its surface", (ink, background) => {
        expect(contrast(color(ink), background)).toBeGreaterThanOrEqual(4.5);
      });

      it("keeps text on the accent readable", () => {
        expect(contrast(color("--sk-on-accent"), color("--sk-accent"))).toBeGreaterThanOrEqual(4.5);
        expect(
          contrast(color("--sk-on-accent"), color("--sk-negative-fill")),
        ).toBeGreaterThanOrEqual(4.5);
      });

      it.each(["positive", "negative", "warning", "info", "neutral"])(
        "%s pills are readable on their tint, on a plain and a selected row",
        (tone) => {
          for (const row of [surface, over(color("--sk-accent-tint"), surface)]) {
            const tint = over(color(`--sk-${tone}-tint`), row);
            expect(contrast(color(`--sk-${tone}-ink`), tint)).toBeGreaterThanOrEqual(4.5);
          }
        },
      );

      it("keeps capsule text readable inside a selected row", () => {
        const row = over(color("--sk-accent-tint"), surface);
        const capsule = over(color("--sk-capsule"), row);
        expect(contrast(color("--sk-ink-secondary"), capsule)).toBeGreaterThanOrEqual(4.5);
      });
    });
  }
});
