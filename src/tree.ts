import type { Tab } from "./tab";

export type ProjectId = number;
export type TabId = number;

export interface Project {
  id: ProjectId;
  name: string;
  folder: string;
}

export interface TreeTab {
  id: TabId;
  /** `null` for a setup tab, such as signing an account in, that belongs to no project. */
  projectId: ProjectId | null;
  parentId: TabId | null;
  title: string;
  launch: Tab;
}

export interface Tree {
  projects: Project[];
  tabs: TreeTab[];
  nextId: number;
}

export interface Row {
  tab: TreeTab;
  sub: boolean;
  /** The last sub-tab under its planner, where the tree line stops. */
  last: boolean;
}

export interface Group {
  project: Project | null;
  rows: Row[];
}

export function emptyTree(): Tree {
  return { projects: [], tabs: [], nextId: 1 };
}

export function folderName(folder: string): string {
  const parts = folder.split(/[\\/]+/).filter((part) => part !== "");
  return parts.at(-1) ?? folder;
}

/** Adding a folder that is already a project returns that project. */
export function addProject(tree: Tree, folder: string): [Tree, ProjectId] {
  const known = tree.projects.find(
    (p) => p.folder.toLowerCase() === folder.toLowerCase(),
  );
  if (known) return [tree, known.id];
  const project = { id: tree.nextId, name: folderName(folder), folder };
  return [
    { ...tree, projects: [...tree.projects, project], nextId: tree.nextId + 1 },
    project.id,
  ];
}

export function defaultTitle(tab: Tab): string {
  return tab.account ? `${tab.label} · ${tab.account.name}` : tab.label;
}

/** A tab that can hold sub-tabs: a Claude Code tab at the top of a project. */
export function isPlanner(tab: TreeTab): boolean {
  return (
    tab.projectId !== null &&
    tab.parentId === null &&
    tab.launch.program === "claude"
  );
}

export function planners(tree: Tree, projectId: ProjectId): TreeTab[] {
  return tree.tabs.filter((t) => t.projectId === projectId && isPlanner(t));
}

function canParent(
  tree: Tree,
  projectId: ProjectId | null,
  parentId: TabId | null,
): boolean {
  if (parentId === null) return true;
  const parent = tree.tabs.find((t) => t.id === parentId);
  return (
    parent !== undefined && isPlanner(parent) && parent.projectId === projectId
  );
}

export function addTab(
  tree: Tree,
  place: { projectId: ProjectId | null; parentId: TabId | null },
  launch: Tab,
): [Tree, TabId] {
  const tab: TreeTab = {
    id: tree.nextId,
    projectId: place.projectId,
    parentId: canParent(tree, place.projectId, place.parentId)
      ? place.parentId
      : null,
    title: defaultTitle(launch),
    launch,
  };
  return [
    { ...tree, tabs: [...tree.tabs, tab], nextId: tree.nextId + 1 },
    tab.id,
  ];
}

export function renameTab(tree: Tree, id: TabId, title: string): Tree {
  const trimmed = title.trim();
  if (trimmed === "") return tree;
  return {
    ...tree,
    tabs: tree.tabs.map((t) => (t.id === id ? { ...t, title: trimmed } : t)),
  };
}

/** A closed planner's sub-tabs stay open and move up to the top of its project. */
export function closeTab(tree: Tree, id: TabId): Tree {
  return {
    ...tree,
    tabs: tree.tabs
      .filter((t) => t.id !== id)
      .map((t) => (t.parentId === id ? { ...t, parentId: null } : t)),
  };
}

/** Moves a tab under another planner, or to the top with `null`; a tab with sub-tabs stays put. */
export function moveTab(tree: Tree, id: TabId, parentId: TabId | null): Tree {
  const tab = tree.tabs.find((t) => t.id === id);
  if (!tab || id === parentId) return tree;
  if (parentId !== null && tree.tabs.some((t) => t.parentId === id))
    return tree;
  if (!canParent(tree, tab.projectId, parentId)) return tree;
  return {
    ...tree,
    tabs: tree.tabs.map((t) => (t.id === id ? { ...t, parentId } : t)),
  };
}

function rowsOf(tabs: TreeTab[], matches: (tab: TreeTab) => boolean): Row[] {
  const rows: Row[] = [];
  for (const top of tabs.filter((t) => t.parentId === null)) {
    const kids = tabs.filter((t) => t.parentId === top.id);
    const shown = kids.filter(matches);
    if (!matches(top) && shown.length === 0) continue;
    rows.push({ tab: top, sub: false, last: false });
    shown.forEach((kid, i) => {
      rows.push({
        tab: kid,
        sub: true,
        last: i === shown.length - 1,
      });
    });
  }
  return rows;
}

/** Setup tabs first, then each project; a search keeps a planner whose sub-tabs match. */
export function groups(
  tree: Tree,
  search: string,
  accountName: (tab: TreeTab) => string,
): Group[] {
  const needle = search.trim().toLowerCase();
  const matches = (tab: TreeTab) =>
    needle === "" ||
    tab.title.toLowerCase().includes(needle) ||
    accountName(tab).toLowerCase().includes(needle);
  const setup = rowsOf(
    tree.tabs.filter((t) => t.projectId === null),
    matches,
  );
  const projects = tree.projects
    .map((project) => ({
      project,
      rows: rowsOf(
        tree.tabs.filter((t) => t.projectId === project.id),
        matches,
      ),
    }))
    .filter((group) => needle === "" || group.rows.length > 0);
  return setup.length > 0
    ? [{ project: null, rows: setup }, ...projects]
    : projects;
}

/** Tab ids in the order the sidebar lists them. */
export function tabOrder(tree: Tree): TabId[] {
  return groups(tree, "", () => "").flatMap((g) => g.rows.map((r) => r.tab.id));
}

/** The tab to show after `id` closes: the next one in the list, else the one before. */
export function neighbour(tree: Tree, id: TabId): TabId | undefined {
  const order = tabOrder(tree);
  const at = order.indexOf(id);
  if (at === -1) return undefined;
  return order[at + 1] ?? order[at - 1];
}

export function stepTab(
  tree: Tree,
  id: TabId | undefined,
  step: 1 | -1,
): TabId | undefined {
  const order = tabOrder(tree);
  if (order.length === 0) return undefined;
  const at = id === undefined ? -1 : order.indexOf(id);
  if (at === -1) return step === 1 ? order[0] : order.at(-1);
  return order[(at + step + order.length) % order.length];
}

/** `next` with each row that did not change taken from `previous`, so a keyed list keeps its DOM. */
export function reuseRows(previous: readonly Group[], next: Group[]): Group[] {
  return next.map((group) => {
    const old = previous.find((g) => g.project === group.project);
    if (!old) return group;
    const rows = group.rows.map(
      (row) =>
        old.rows.find(
          (r) => r.tab === row.tab && r.sub === row.sub && r.last === row.last,
        ) ?? row,
    );
    const same =
      rows.length === old.rows.length &&
      rows.every((r, i) => r === old.rows[i]);
    return same ? old : { ...group, rows };
  });
}
