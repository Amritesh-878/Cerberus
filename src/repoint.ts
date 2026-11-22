import { isRemoved, type Profiles, sameFolder } from "./accounts";
import { moveToAccount, type Tree } from "./tree";

/** Tabs on a removed account whose folder an active account uses go to that account: their chats are already there. */
export function repointed(tree: Tree, profiles: Profiles): Tree {
  let next = tree;
  for (const tab of tree.tabs) {
    const id = tab.launch.account?.id;
    if (id == null || !isRemoved(profiles, id)) continue;
    const gone = profiles.retired.find((a) => a.id === id);
    const twin =
      gone &&
      profiles.accounts.find((a) => sameFolder(a.configDir, gone.configDir));
    if (twin) next = moveToAccount(next, tab.id, twin);
  }
  return next;
}
