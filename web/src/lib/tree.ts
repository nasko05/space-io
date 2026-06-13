import type { TreeFolder, TreeNode } from '../api/client';

/** A folder flattened out of the vault tree, with its nesting depth so the
 *  picker can indent it. `path: ''` is the synthetic space-root entry. */
export interface FolderEntry {
  path: string;
  label: string;
  depth: number;
}

/** Flatten the folder hierarchy into a name-sorted, depth-indexed list led by a
 *  space-root entry. Shared by the Create-folder and Move dialogs, which both
 *  render the same parent-folder picker. */
export function collectFolders(tree: TreeNode[]): FolderEntry[] {
  const out: FolderEntry[] = [{ path: '', label: '/ (space root)', depth: 0 }];
  const walk = (nodes: TreeNode[], depth: number) => {
    const folders = nodes.filter((node): node is TreeFolder => node.type === 'folder');
    folders.sort((a, b) => a.name.localeCompare(b.name));
    for (const folder of folders) {
      out.push({ path: folder.path, label: folder.name, depth });
      walk(folder.children, depth + 1);
    }
  };
  walk(tree, 1);
  return out;
}
