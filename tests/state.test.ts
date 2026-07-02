// SPDX-License-Identifier: Apache-2.0
import { beforeEach, describe, expect, it } from "vitest";
import {
  addPaths,
  capabilities,
  clearQueue,
  effectiveQualityFor,
  fmtSize,
  formatFromExt,
  formatLabel,
  qualityFloorFor,
  queue,
  settings,
} from "../src/lib/state.svelte";

describe("state helpers", () => {
  beforeEach(() => {
    clearQueue();
    capabilities.readable = ["jpeg", "png", "webp", "avif"];
    capabilities.writable = ["jpeg", "png", "webp", "avif"];
    capabilities.lossless = ["png", "webp"];
    capabilities.heic = false;
    capabilities.codecProviders = [];
    settings.quality = 80;
    settings.lossless = false;
    settings.jpegQualityFloor = 30;
    settings.webpQualityFloor = 30;
    settings.avifQualityFloor = 30;
  });

  it("maps extensions and labels through the capability model", () => {
    expect(formatFromExt("JPG")).toBe("jpeg");
    expect(formatFromExt("unknown")).toBeNull();
    expect(formatLabel("webp")).toBe("WebP");
  });

  it("adds readable paths and reports duplicates/skips", () => {
    const result = addPaths(["/tmp/a.png", "/tmp/a.png", "/tmp/readme.txt"]);

    expect(result).toEqual({ added: 1, duplicates: 1, skipped: 1 });
    expect(queue).toHaveLength(1);
    expect(queue[0]?.name).toBe("a.png");
  });

  it("formats byte sizes for queue summaries", () => {
    expect(fmtSize(0)).toBe("0 B");
    expect(fmtSize(1536)).toBe("1.5 KB");
  });

  it("applies per-format quality floors only to lossy targets", () => {
    settings.quality = 18;
    settings.jpegQualityFloor = 65;
    settings.webpQualityFloor = 29;
    settings.avifQualityFloor = 45;

    expect(qualityFloorFor("jpeg")).toBe(65);
    expect(effectiveQualityFor("jpeg")).toBe(65);
    expect(effectiveQualityFor("webp")).toBe(18);
    expect(effectiveQualityFor("avif")).toBe(45);
    expect(effectiveQualityFor("png")).toBe(18);

    settings.lossless = true;
    expect(effectiveQualityFor("webp")).toBe(18);
  });
});
