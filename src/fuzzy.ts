// The same scoring as the core's chat names, so tabs and chats rank alike.
const PER_LETTER = 16;
const AT_START = 12;
const AT_WORD = 8;
const IN_A_RUN = 6;

/** A name's match: its score and the matched letters' positions, in characters. */
export interface Fuzzy {
  score: number;
  marks: number[];
}

/** A character range to highlight: start and length. */
export type Mark = [number, number];

/** Every word (lower case) found in `name` with its letters in order, fzf-style; `null` when any word isn't. */
export function fuzzyWords(words: string[], name: string): Fuzzy | null {
  if (words.length === 0) return null;
  const chars = Array.from(name);
  let score = 0;
  const marks = new Set<number>();
  for (const word of words) {
    const one = oneWord(word, chars);
    if (!one) return null;
    score += one.score;
    for (const at of one.marks) marks.add(at);
  }
  return { score, marks: [...marks].sort((a, b) => a - b) };
}

function lower(c: string): string {
  return c.toLowerCase();
}

// The better of the first match found forward and that match narrowed backward, as fzf narrows it.
function oneWord(word: string, name: string[]): Fuzzy | null {
  const wanted = Array.from(word).filter((c) => c.trim() !== "");
  if (wanted.length === 0) return null;
  const forward: number[] = [];
  name.forEach((c, at) => {
    if (forward.length < wanted.length && lower(c) === wanted[forward.length])
      forward.push(at);
  });
  if (forward.length < wanted.length) return null;
  const end = forward[forward.length - 1] ?? 0;
  const narrowed: number[] = [];
  for (let at = end; at >= 0; at--) {
    const want = wanted[wanted.length - 1 - narrowed.length];
    if (narrowed.length < wanted.length && lower(name[at] ?? "") === want)
      narrowed.push(at);
  }
  narrowed.reverse();
  const a = scored(name, forward);
  const b = scored(name, narrowed);
  return b.score > a.score ? b : a;
}

function scored(name: string[], marks: number[]): Fuzzy {
  let score = 0;
  marks.forEach((at, index) => {
    score += PER_LETTER;
    if (at === 0) score += AT_START;
    else if (isWordStart(name, at)) score += AT_WORD;
    if (index > 0 && marks[index - 1] === at - 1) score += IN_A_RUN;
  });
  const first = marks[0] ?? 0;
  const last = marks[marks.length - 1] ?? 0;
  // Letters skipped inside the run cost a point each, so a tighter match wins.
  score -= last - first + 1 - marks.length;
  return { score: Math.max(1, score), marks };
}

// After a separator, or an upper-case letter after a lower-case one, as in `camelCase`.
function isWordStart(name: string[], at: number): boolean {
  const before = name[at - 1] ?? "";
  const now = name[at] ?? "";
  const alnum = /[\p{L}\p{N}]/u;
  return (
    !alnum.test(before) ||
    (before === before.toLowerCase() &&
      before !== before.toUpperCase() &&
      now === now.toUpperCase() &&
      now !== now.toLowerCase())
  );
}

/** Matched letter positions as ranges, neighbours joined. */
export function letterRanges(marks: readonly number[]): Mark[] {
  const ranges: Mark[] = [];
  for (const at of marks) {
    const last = ranges[ranges.length - 1];
    if (last && last[0] + last[1] === at) last[1] += 1;
    else ranges.push([at, 1]);
  }
  return ranges;
}

/** The typed text's words, lower case, as both searches split them. */
export function wordsOf(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}
