import type { TabId } from "./tree";

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

/** Tabs whose title, project, folder or account hold every typed word, in the order given; no words lists them all. */
export function matchTabs(
  entries: readonly TabEntry[],
  query: string,
): TabEntry[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...entries];
  return entries.filter((entry) => {
    const text = [entry.title, entry.project, entry.folder, entry.account]
      .filter((part): part is string => part !== null)
      .join(" ")
      .toLowerCase();
    return words.every((word) => text.includes(word));
  });
}
