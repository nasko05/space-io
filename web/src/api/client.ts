export interface AuthStatus {
  /** At least one user has registered. */
  any_users: boolean;
  /** The current cookie maps to a live session. */
  unlocked: boolean;
  /** Display name of the unlocked user (empty when locked). */
  owner: string;
  /** Email of the unlocked user (empty when locked). */
  email: string;
  has_passkey: boolean;
}

export interface InitResult {
  /** UUID assigned to the new user (i.e. the on-disk folder name). */
  user_uuid: string;
}

export interface PasskeyInfo {
  credential_id_b64: string;
  prf_salt_b64: string;
  wrapped_passphrase_b64: string;
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

export interface WriteResult {
  path: string;
  updated: string;
}

export interface CreateResult {
  path: string;
}

export interface ExcerptItem {
  title: string | null;
  excerpt: string;
}

export type ExcerptMap = Record<string, ExcerptItem>;

export interface SearchHit {
  path: string;
  title: string | null;
  snippet: string;
}

export interface UploadResultItem {
  path: string;
  size: number;
}

export interface HistoryEntry {
  commit: string;
  message: string;
  author: string;
  when: string;
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
  async unlock(email: string, passphrase: string): Promise<void> {
    await json(
      await fetch('/api/auth/unlock', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ email, passphrase }),
      }),
    );
  },
  async init(email: string, passphrase: string, owner?: string): Promise<InitResult> {
    return json(
      await fetch('/api/auth/init', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ email, passphrase, owner: owner ?? null }),
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
  async write(path: string, content: string, message?: string): Promise<WriteResult> {
    return json(
      await fetch('/api/files/write', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ path, content, message }),
      }),
    );
  },
  async create(folder: string, title?: string): Promise<CreateResult> {
    return json(
      await fetch('/api/files/create', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ folder, title: title ?? null }),
      }),
    );
  },
  async excerpts(): Promise<{ excerpts: ExcerptMap }> {
    return json(await fetch('/api/files/excerpts', { credentials: 'same-origin' }));
  },
  async search(q: string): Promise<{ hits: SearchHit[] }> {
    return json(
      await fetch(`/api/search?q=${encodeURIComponent(q)}`, {
        credentials: 'same-origin',
      }),
    );
  },
  async upload(
    folder: string,
    files: File[],
    onProgress?: (loaded: number, total: number) => void,
  ): Promise<{ files: UploadResultItem[] }> {
    const body = new FormData();
    body.append('folder', folder);
    for (const f of files) {
      body.append('file', f, f.name);
    }
    if (onProgress) {
      return new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open('POST', '/api/files/upload', true);
        xhr.withCredentials = true;
        xhr.upload.onprogress = (e) => {
          if (e.lengthComputable) onProgress(e.loaded, e.total);
        };
        xhr.onload = () => {
          if (xhr.status >= 200 && xhr.status < 300) {
            try {
              resolve(JSON.parse(xhr.responseText));
            } catch (e) {
              reject(e);
            }
          } else {
            try {
              const body = JSON.parse(xhr.responseText);
              reject(
                new ApiError(
                  xhr.status,
                  body?.error?.code ?? 'unknown',
                  body?.error?.message ?? xhr.statusText,
                ),
              );
            } catch {
              reject(new ApiError(xhr.status, 'unknown', xhr.statusText));
            }
          }
        };
        xhr.onerror = () => reject(new ApiError(0, 'network', 'upload failed'));
        xhr.send(body);
      });
    }
    const res = await fetch('/api/files/upload', {
      method: 'POST',
      credentials: 'same-origin',
      body,
    });
    return json(res);
  },
  downloadUrl(path: string): string {
    return `/api/files/download?path=${encodeURIComponent(path)}`;
  },
  async history(path: string): Promise<{ entries: HistoryEntry[] }> {
    return json(
      await fetch(`/api/files/history?path=${encodeURIComponent(path)}`, {
        credentials: 'same-origin',
      }),
    );
  },
  async passkeyInfo(email: string): Promise<PasskeyInfo | null> {
    const res = await fetch(`/api/auth/passkey/info?email=${encodeURIComponent(email)}`, {
      credentials: 'same-origin',
    });
    if (res.status === 404) return null;
    return json(res);
  },
  async registerPasskey(payload: PasskeyInfo): Promise<void> {
    await json(
      await fetch('/api/auth/passkey/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify(payload),
      }),
    );
  },
  async deletePasskey(): Promise<void> {
    await json(
      await fetch('/api/auth/passkey', {
        method: 'DELETE',
        credentials: 'same-origin',
      }),
    );
  },
  async move(from: string, to: string): Promise<{ path: string; is_directory: boolean }> {
    return json(
      await fetch('/api/files/move', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ from, to }),
      }),
    );
  },
  async moveBulk(
    moves: { from: string; to: string }[],
  ): Promise<{ results: { path: string; is_directory: boolean }[] }> {
    return json(
      await fetch('/api/files/move/bulk', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ moves }),
      }),
    );
  },
  async deleteFile(path: string): Promise<{ trash_path: string }> {
    return json(
      await fetch('/api/files/delete', {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ path }),
      }),
    );
  },
  async deleteFilesBulk(paths: string[]): Promise<{ results: { trash_path: string }[] }> {
    return json(
      await fetch('/api/files/delete/bulk', {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ paths }),
      }),
    );
  },
  async mkdir(path: string): Promise<void> {
    await json(
      await fetch('/api/files/mkdir', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ path }),
      }),
    );
  },
  async meta(): Promise<{ meta: MetaMap }> {
    return json(await fetch('/api/files/meta', { credentials: 'same-origin' }));
  },
  async setTags(path: string, tags: string[]): Promise<void> {
    await json(
      await fetch('/api/files/meta', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ path, tags }),
      }),
    );
  },
  async setTagsBulk(updates: { path: string; tags: string[] }[]): Promise<void> {
    await json(
      await fetch('/api/files/meta/bulk', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ updates }),
      }),
    );
  },
};

export interface MetaItem {
  tags: string[];
}

export type MetaMap = Record<string, MetaItem>;

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

export function flattenFiles(tree: TreeNode[]): TreeFile[] {
  const out: TreeFile[] = [];
  const walk = (nodes: TreeNode[]) => {
    for (const n of nodes) {
      if (n.type === 'file') out.push(n);
      else walk(n.children);
    }
  };
  walk(tree);
  return out;
}

export function topLevelFolders(tree: TreeNode[]): TreeFolder[] {
  return tree.filter((n): n is TreeFolder => n.type === 'folder');
}
