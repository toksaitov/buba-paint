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
    expect(html).toContain('rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png?v=20260509-pwa2"');
    expect(html).toContain('rel="mask-icon" href="/mask-icon.svg?v=20260509-pwa2" color="#2ea44f"');
    expect(html).toContain('rel="manifest" href="/site.webmanifest?v=20260509-pwa2"');
  });
});
