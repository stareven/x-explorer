import { pinyin } from "pinyin-pro";

export interface SearchIndex {
  raw: string;
  fullPinyin: string;
  initials: string;
  combined: string;
}

export interface SearchMatch {
  tier: 0 | 1 | 2 | 3;
  position: number;
  length: number;
}

export function normalizeSearchQuery(query: string): string {
  return query.trim().toLowerCase().replace(/\s+/g, "");
}

export function buildSearchIndex(...parts: string[]): SearchIndex {
  const raw = parts.join(" ").toLowerCase().replace(/\s+/g, "");
  const fullPinyin = parts
    .map((part) => pinyin(part, { toneType: "none", type: "array" }).join(""))
    .join("")
    .toLowerCase();
  const initials = parts
    .map((part) => pinyin(part, { toneType: "none", pattern: "first", type: "array" }).join(""))
    .join("")
    .toLowerCase();

  return {
    raw,
    fullPinyin,
    initials,
    combined: `${raw}${fullPinyin}${initials}`,
  };
}

function subsequenceMatch(text: string, query: string): SearchMatch | null {
  let queryIndex = 0;
  let firstPosition = -1;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === query[queryIndex]) {
      if (firstPosition === -1) firstPosition = index;
      queryIndex += 1;
      if (queryIndex === query.length) return { tier: 3, position: firstPosition, length: text.length };
    }
  }
  return null;
}

function substringMatch(text: string, query: string, tier: SearchMatch["tier"]): SearchMatch | null {
  const position = text.indexOf(query);
  return position === -1 ? null : { tier, position, length: text.length };
}

export function rankSearchIndex(searchIndex: SearchIndex, query: string): SearchMatch | null {
  const normalizedQuery = normalizeSearchQuery(query);
  if (!normalizedQuery) return { tier: 0, position: 0, length: 0 };

  const exactMatches = [
    substringMatch(searchIndex.raw, normalizedQuery, 0),
    substringMatch(searchIndex.fullPinyin, normalizedQuery, 1),
    substringMatch(searchIndex.initials, normalizedQuery, 2),
  ].filter((match): match is SearchMatch => match != null);
  if (exactMatches.length > 0) {
    return exactMatches.reduce((best, current) => (compareSearchMatches(current, best) < 0 ? current : best));
  }

  const subsequenceMatches = [
    subsequenceMatch(searchIndex.raw, normalizedQuery),
    subsequenceMatch(searchIndex.fullPinyin, normalizedQuery),
    subsequenceMatch(searchIndex.initials, normalizedQuery),
  ].filter((match): match is SearchMatch => match != null);

  if (subsequenceMatches.length === 0) return null;
  return subsequenceMatches.reduce((best, current) => (compareSearchMatches(current, best) < 0 ? current : best));
}

export function matchesSearchIndex(searchIndex: SearchIndex, query: string): boolean {
  return rankSearchIndex(searchIndex, query) != null;
}

export function compareSearchMatches(left: SearchMatch, right: SearchMatch): number {
  return left.tier - right.tier || left.position - right.position || left.length - right.length;
}
