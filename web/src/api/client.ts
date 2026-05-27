export interface AuthStatus {
  unlocked: boolean;
  owner: string;
}

export interface TreeFile {
  type: 'file';
  name: string;
  path: string;
  kind: string;
  updated: string;
  size: number;
}

export interface TreeFolder {
  type: 'folder';
  name: string;
  path: string;
  children: TreeNode[];
}

export type TreeNode = TreeFile | TreeFolder;

export interface ReadFile {
  path: string;
  content: string;
  updated: string | null;
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message);
  }
}

async function json<T>(res: Response): Promise<T> {
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  if (!res.ok) {
    let code = 'unknown';
    let message = text || res.statusText;
    try {
      const body = JSON.parse(text);
      code = body?.error?.code ?? code;
      message = body?.error?.message ?? message;
    } catch {
      // leave defaults
    }
    throw new ApiError(res.status, code, message);
  }
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

export const api = {
  async status(): Promise<AuthStatus> {
    return json(await fetch('/api/auth/status', { credentials: 'same-origin' }));
  },
  async unlock(passphrase: string): Promise<void> {
    await json(
      await fetch('/api/auth/unlock', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ passphrase }),
      }),
    );
  },
  async lock(): Promise<void> {
    await json(
      await fetch('/api/auth/lock', {
        method: 'POST',
        credentials: 'same-origin',
      }),
    );
  },
  async tree(): Promise<{ tree: TreeNode[] }> {
    return json(await fetch('/api/files/tree', { credentials: 'same-origin' }));
  },
  async read(path: string): Promise<ReadFile> {
    return json(
      await fetch(`/api/files/read?path=${encodeURIComponent(path)}`, {
        credentials: 'same-origin',
      }),
    );
  },
};

export function firstMarkdownLeaf(tree: TreeNode[]): TreeFile | null {
  for (const node of tree) {
    if (node.type === 'file' && node.kind === 'md') return node;
    if (node.type === 'folder') {
      const hit = firstMarkdownLeaf(node.children);
      if (hit) return hit;
    }
  }
  return null;
}
