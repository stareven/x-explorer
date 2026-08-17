import { describe, expect, it } from "vitest";
import { buildSearchIndex, compareSearchMatches, matchesSearchIndex, rankSearchIndex } from "./search";

describe("search helpers", () => {
  it("matches non-contiguous pinyin initials as a subsequence", () => {
    const index = buildSearchIndex("照片备份");

    expect(matchesSearchIndex(index, "zpf")).toBe(true);
  });

  it("does not match subsequences across different search segments", () => {
    const index = buildSearchIndex("照片备份");

    expect(matchesSearchIndex(index, "fz")).toBe(false);
  });

  it("ranks raw, full pinyin, initials, and subsequence matches in priority order", () => {
    const raw = rankSearchIndex(buildSearchIndex("照片"), "照片");
    const fullPinyin = rankSearchIndex(buildSearchIndex("照片备份"), "zhao");
    const initials = rankSearchIndex(buildSearchIndex("照片备份"), "zpb");
    const subsequence = rankSearchIndex(buildSearchIndex("照片备份"), "zpf");

    expect(raw?.tier).toBe(0);
    expect(fullPinyin?.tier).toBe(1);
    expect(initials?.tier).toBe(2);
    expect(subsequence?.tier).toBe(3);
  });

  it("sorts earlier and shorter matches first within the same tier", () => {
    const earlier = rankSearchIndex(buildSearchIndex("照片"), "zhao")!;
    const later = rankSearchIndex(buildSearchIndex("我的照片"), "zhao")!;
    const shorter = rankSearchIndex(buildSearchIndex("照片"), "zhao")!;
    const longer = rankSearchIndex(buildSearchIndex("照片备份"), "zhao")!;

    expect(compareSearchMatches(earlier, later)).toBeLessThan(0);
    expect(compareSearchMatches(shorter, longer)).toBeLessThan(0);
  });
});
