import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// Convert the CSS OKLCH tokens into linear sRGB before measuring WCAG luminance.
function luminance(value: string): number {
  const parts = value.match(/oklch\(([\d.]+) ([\d.]+) ([\d.]+)\)/);
  assert.ok(parts, `Expected an opaque OKLCH token: ${value}`);
  const [, light, chroma, hue] = parts;
  const L = Number(light), C = Number(chroma), h = Number(hue) * Math.PI / 180;
  const a = C * Math.cos(h), b = C * Math.sin(h);
  const l = (L + 0.3963377774*a + 0.2158037573*b)**3;
  const m = (L - 0.1055613458*a - 0.0638541728*b)**3;
  const s = (L - 0.0894841775*a - 1.291485548*b)**3;
  const rgb = [4.0767416621*l - 3.3077115913*m + 0.2309699292*s, -1.2684380046*l + 2.6097574011*m - 0.3413193965*s, -0.0041960863*l - 0.7034186147*m + 1.707614701*s].map(v => Math.min(1, Math.max(0,v)));
  return 0.2126*rgb[0]+0.7152*rgb[1]+0.0722*rgb[2];
}
test("contrast conversion matches black and white", () => {
  assert.equal(luminance("oklch(0 0 0)"),0);
  assert.ok(Math.abs(luminance("oklch(1 0 0)")-1)<0.00001);
});
const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");
for (const theme of [":root", ".dark"]) {
  test(`${theme} text and status tokens meet WCAG AA`, () => {
    const block = css.slice(css.indexOf(`${theme} {`)).split("}")[0];
    const tokens = Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(oklch\([^;]+\));/g)].map(m=>[m[1],m[2]]));
    const pairs = [["foreground","background"],["card-foreground","card"],["muted-foreground","muted"],["muted-foreground","card"],["primary-foreground","primary"],["sidebar-foreground","sidebar"],["status-critical-foreground","status-critical"],...["status-ok","status-warning","status-critical","status-info"].map(t=>[t,"card"])];
    for (const [fg,bg] of pairs) {
      const a=luminance(tokens[fg]), b=luminance(tokens[bg]);
      const ratio=(Math.max(a,b)+0.05)/(Math.min(a,b)+0.05);
      assert.ok(ratio >= 4.5, `${theme} ${fg} on ${bg}: ${ratio.toFixed(2)}:1`);
    }
  });
}
