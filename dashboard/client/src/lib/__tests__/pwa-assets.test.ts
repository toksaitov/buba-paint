import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";
import { describe, expect, test } from "vitest";

interface PngInfo {
  width: number;
  height: number;
  transparentPixels: number;
  cornerAlphas: number[];
  pixelAt: (x: number, y: number) => [number, number, number, number];
}

const pngSignature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const publicDir = join(process.cwd(), "public");
const indexPath = join(process.cwd(), "index.html");
const cssPath = join(process.cwd(), "src/index.css");

function paeth(a: number, b: number, c: number) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

function readPngInfo(name: string): PngInfo {
  const buffer = readFileSync(join(publicDir, name));
  expect(buffer.subarray(0, 8)).toEqual(pngSignature);

  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat: Buffer[] = [];

  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      expect(data[8]).toBe(8);
      colorType = data[9];
    }
    if (type === "IDAT") idat.push(Buffer.from(data));
    offset += 12 + length;
  }

  expect(colorType).toBe(6);
  const raw = inflateSync(Buffer.concat(idat));
  const bytesPerPixel = 4;
  const rowLength = width * bytesPerPixel;
  let src = 0;
  let transparentPixels = 0;
  const pixels = new Uint8Array(width * height * bytesPerPixel);
  let previous = new Uint8Array(rowLength);

  for (let y = 0; y < height; y += 1) {
    const filter = raw[src];
    src += 1;
    const current = new Uint8Array(rowLength);
    for (let x = 0; x < rowLength; x += 1) {
      const value = raw[src + x];
      const left = x >= bytesPerPixel ? current[x - bytesPerPixel] : 0;
      const up = previous[x] ?? 0;
      const upperLeft = x >= bytesPerPixel ? previous[x - bytesPerPixel] : 0;
      current[x] =
        filter === 0
          ? value
          : filter === 1
            ? (value + left) & 255
            : filter === 2
              ? (value + up) & 255
              : filter === 3
                ? (value + Math.floor((left + up) / 2)) & 255
                : (value + paeth(left, up, upperLeft)) & 255;
    }
    pixels.set(current, y * rowLength);
    for (let x = 3; x < rowLength; x += bytesPerPixel) {
      if (current[x] < 255) transparentPixels += 1;
    }
    previous = current;
    src += rowLength;
  }

  const pixelAt = (x: number, y: number): [number, number, number, number] => {
    const offset = (y * width + x) * bytesPerPixel;
    return [pixels[offset], pixels[offset + 1], pixels[offset + 2], pixels[offset + 3]];
  };
  const alphaAt = (x: number, y: number) => pixelAt(x, y)[3];
  return {
    width,
    height,
    transparentPixels,
    cornerAlphas: [
      alphaAt(0, 0),
      alphaAt(width - 1, 0),
      alphaAt(0, height - 1),
      alphaAt(width - 1, height - 1),
    ],
    pixelAt,
  };
}

describe("PWA assets", () => {
  test("browser favicon PNG keeps transparent corners and a white rounded border", () => {
    const icon = readPngInfo("icon-32x32.png");
    expect(icon.width).toBe(32);
    expect(icon.height).toBe(32);
    expect(icon.transparentPixels).toBeGreaterThan(0);
    expect(icon.cornerAlphas).toEqual([0, 0, 0, 0]);
    expect(icon.pixelAt(16, 2).slice(0, 3).every((channel) => channel >= 235)).toBe(true);
  });

  test("browser tab favicon provides vector artwork and multi-size raster fallbacks", () => {
    const icon16 = readPngInfo("favicon-16x16.png");
    const icon32 = readPngInfo("favicon-32x32.png");
    const icon48 = readPngInfo("favicon-48x48.png");
    const icon64 = readPngInfo("favicon-64x64.png");
    expect(icon16.width).toBe(16);
    expect(icon16.height).toBe(16);
    expect(icon32.width).toBe(32);
    expect(icon32.height).toBe(32);
    expect(icon48.width).toBe(48);
    expect(icon48.height).toBe(48);
    expect(icon64.width).toBe(64);
    expect(icon64.height).toBe(64);
    expect(icon32.transparentPixels).toBeGreaterThan(0);
    expect(icon32.cornerAlphas).toEqual([0, 0, 0, 0]);
    expect(icon32.pixelAt(16, 3).slice(0, 3).every((channel) => channel >= 220)).toBe(true);
    expect(icon32.pixelAt(16, 16).slice(0, 3)).toEqual([0, 0, 0]);
    const svg = readFileSync(join(publicDir, "favicon.svg"), "utf8");
    expect(svg).toContain('viewBox="0 0 64 64"');
    expect(svg).toContain('fill="#000000"');
    expect(svg).toContain('stroke="white"');
  });

  test("apple touch icon is opaque with visible white border artwork", () => {
    const icon = readPngInfo("apple-touch-icon.png");
    expect(icon.width).toBe(180);
    expect(icon.height).toBe(180);
    expect(icon.transparentPixels).toBe(0);
    expect(icon.cornerAlphas).toEqual([255, 255, 255, 255]);
    expect(icon.pixelAt(90, 5).slice(0, 3).every((channel) => channel >= 235)).toBe(true);
    expect(icon.pixelAt(90, 90).slice(0, 3)).toEqual([0, 0, 0]);
  });

  test("manifest has normal and maskable install icons", () => {
    const manifest = JSON.parse(
      readFileSync(join(publicDir, "site.webmanifest"), "utf8"),
    ) as {
      id: string;
      display: string;
      icons: Array<{ src: string; sizes: string; purpose?: string }>;
    };

    expect(manifest.id).toBe("/");
    expect(manifest.display).toBe("standalone");
    expect(
      manifest.icons.some(
        (icon) => icon.src === "/icon-192x192.png" && icon.purpose === "any",
      ),
    ).toBe(true);
    expect(
      manifest.icons.some(
        (icon) =>
          icon.src === "/icon-192x192-maskable.png" && icon.purpose === "maskable",
      ),
    ).toBe(true);
    expect(
      manifest.icons.some(
        (icon) =>
          icon.src === "/icon-512x512-maskable.png" && icon.purpose === "maskable",
      ),
    ).toBe(true);

    for (const icon of manifest.icons) {
      const fileName = icon.src.replace("/", "");
      expect(existsSync(join(publicDir, fileName))).toBe(true);
      const [width, height] = icon.sizes.split("x").map(Number);
      const png = readPngInfo(fileName);
      expect(png.width).toBe(width);
      expect(png.height).toBe(height);
    }
  });

  test("index exposes standalone mobile app metadata", () => {
    const html = readFileSync(indexPath, "utf8");
    expect(html).toContain('name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover"');
    expect(html).toContain('name="apple-mobile-web-app-capable" content="yes"');
    expect(html).toContain('name="mobile-web-app-capable" content="yes"');
    expect(html).toContain('name="apple-mobile-web-app-title" content="Buba"');
    expect(html).toContain('rel="icon" type="image/svg+xml" href="/favicon.svg?v=20260510-favicon3"');
    expect(html).toContain('rel="icon" type="image/png" sizes="64x64" href="/favicon-64x64.png?v=20260510-favicon3"');
    expect(html).toContain('rel="icon" type="image/png" sizes="48x48" href="/favicon-48x48.png?v=20260510-favicon3"');
    expect(html).toContain('rel="icon" type="image/png" sizes="32x32" href="/favicon-32x32.png?v=20260510-favicon3"');
    expect(html).toContain('rel="icon" type="image/png" sizes="16x16" href="/favicon-16x16.png?v=20260510-favicon3"');
    expect(html).toContain('rel="icon" href="/favicon.ico?v=20260510-favicon3" sizes="any"');
    expect(html).not.toContain("favicon-bordered.svg");
    expect(html).toContain('rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png?v=20260510-favicon3"');
    expect(html).toContain('rel="mask-icon" href="/mask-icon.svg?v=20260510-favicon3" color="#2ea44f"');
    expect(html).toContain('rel="manifest" href="/site.webmanifest?v=20260510-favicon3"');
  });

  test("safe-area CSS keeps tablet home-screen app chrome clear of the status bar", () => {
    const css = readFileSync(cssPath, "utf8");
    expect(css).toContain("--app-safe-top: env(safe-area-inset-top, 0px)");
    expect(css).toContain(".app-sidebar");
    expect(css).toContain("@media (min-width: 768px) and (hover: hover) and (pointer: fine)");
    expect(css).not.toContain("@media (min-width: 768px) {\n  .app-header");
  });
});
