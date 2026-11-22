import { fuzzyWords, letterRanges, type Mark, wordsOf } from "./fuzzy";
import { folderName, type TabId } from "./tree";

/** One open tab as the search lists it. */
export interface TabEntry {
  id: TabId;
  title: string;
  /** The project's name, or "Setup" for a setup tab. */
  project: string;
  folder: string | null;
  account: string | null;
  accountId: number | null;
  program: string;
}

/** A tab that matched, with the title's matched letters to highlight. */
export interface TabHit {
  tab: TabEntry;
  marks: Mark[];
}

/** Tabs where each word's letters appear in order in the title, project, folder name or account, best first; no words lists them all. */
export function matchTabs(
  entries: readonly TabEntry[],
  query: string,
): TabHit[] {
  const words = wordsOf(query);
  if (words.length === 0) return entries.map((tab) => ({ tab, marks: [] }));
  const hits: (TabHit & { score: number; index: number })[] = [];
  entries.forEach((tab, index) => {
    const fields = [
      tab.title,
      tab.project,
      tab.folder === null ? null : folderName(tab.folder),
      tab.account,
    ];
    let score = 0;
    const titleMarks: number[] = [];
    for (const word of words) {
      let best: { score: number; marks: number[]; field: number } | null = null;
      for (const [field, text] of fields.entries()) {
        const found = text === null ? null : fuzzyWords([word], text);
        if (found && (best === null || found.score > best.score))
          best = { ...found, field };
      }
      if (best === null) return;
      score += best.score;
      if (best.field === 0) titleMarks.push(...best.marks);
    }
    const unique = [...new Set(titleMarks)].sort((a, b) => a - b);
    hits.push({ tab, marks: letterRanges(unique), score, index });
  });
  return hits
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map(({ tab, marks }) => ({ tab, marks }));
}
