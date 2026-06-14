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

export type AgentRole = 'system' | 'user' | 'assistant' | 'tool';

export interface AgentToolCall {
  id: string;
  type?: string;
  function: { name: string; arguments: string };
}

/** OpenAI-compatible chat message. The browser owns the running transcript and
 *  echoes it back on every turn — the server is stateless between turns. */
export interface AgentMessage {
  role: AgentRole;
  content?: string | null;
  tool_calls?: AgentToolCall[];
  tool_call_id?: string;
  name?: string;
}

/** A vault change the model proposed and the user must approve before it is
 *  applied. `tool` selects which endpoint the browser calls on approval. */
export interface AgentPendingAction {
  tool_call_id: string;
  tool: string;
  args: Record<string, unknown>;
  summary: string;
}

export interface AgentChatResponse {
  messages: AgentMessage[];
  assistant_text: string | null;
  pending_actions: AgentPendingAction[];
  done: boolean;
}

export interface AgentStatus {
  /** A provider key is present server-side; the assistant is usable. */
  configured: boolean;
  /** The chat model id in use. */
  model: string;
  web_search: 'brave' | 'builtin' | 'off';
}

export class ApiError extends Error {
  constructor(public readonly status: number, public readonly code: string, message: string) {
    super(message);
  }
}

const JSON_HEADERS = { 'Content-Type': 'application/json' };

async function readJson<T>(res: Response): Promise<T> {
  if (res.status === 204) { return undefined as T; }
  const text = await res.text();
  if (!res.ok) {
    let code = 'unknown';
    let message = text || res.statusText;
    try {
      const body = JSON.parse(text);
      code = body?.error?.code ?? code;
      message = body?.error?.message ?? message;
    } catch {
      message = text || res.statusText;
    }
    throw new ApiError(res.status, code, message);
  }
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

/** Fetch `url` (no body by default) and parse the JSON response. */
async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  return readJson<T>(await fetch(url, { credentials: 'same-origin', ...init }));
}

/** Send a JSON body via `method` and parse the JSON response. */
async function sendJson<T>(method: string, url: string, body: unknown): Promise<T> {
  return requestJson<T>(url, { method, headers: JSON_HEADERS, body: JSON.stringify(body) });
}

export const api = {
  async status(): Promise<AuthStatus> {
    return requestJson('/api/auth/status');
  },
  async unlock(email: string, passphrase: string): Promise<void> {
    await sendJson('POST', '/api/auth/unlock', { email, passphrase });
  },
  async init(email: string, passphrase: string, owner?: string): Promise<InitResult> {
    return sendJson('POST', '/api/auth/init', { email, passphrase, owner: owner ?? null });
  },
  async lock(): Promise<void> {
    await requestJson('/api/auth/lock', { method: 'POST' });
  },
  async tree(): Promise<{ tree: TreeNode[] }> {
    return requestJson('/api/files/tree');
  },
  async read(path: string): Promise<ReadFile> {
    return requestJson(`/api/files/read?path=${encodeURIComponent(path)}`);
  },
  /** Autosave: persist editor content without a history entry; only an explicit
   *  checkpoint becomes a commit. */
  async saveDraft(path: string, content: string): Promise<WriteResult> {
    return sendJson('PUT', '/api/files/write', { path, content });
  },
  /** Checkpoint: persist `content` and record a labelled point in history; a
   *  blank or omitted `message` falls back to a server default. */
  async checkpoint(path: string, content: string, message?: string): Promise<WriteResult> {
    return sendJson('POST', '/api/files/checkpoint', { path, content, message });
  },
  async create(folder: string, title?: string): Promise<CreateResult> {
    return sendJson('POST', '/api/files/create', { folder, title: title ?? null });
  },
  async excerpts(): Promise<{ excerpts: ExcerptMap }> {
    return requestJson('/api/files/excerpts');
  },
  async search(q: string): Promise<{ hits: SearchHit[] }> {
    return requestJson(`/api/search?q=${encodeURIComponent(q)}`);
  },
  async upload(
    folder: string,
    files: File[],
    onProgress?: (loaded: number, total: number) => void,
  ): Promise<{ files: UploadResultItem[] }> {
    const body = new FormData();
    body.append('folder', folder);
    for (const file of files) {
      body.append('file', file, file.name);
    }
    if (onProgress) {
      return uploadWithProgress(body, onProgress);
    }
    return requestJson('/api/files/upload', { method: 'POST', body });
  },
  downloadUrl(path: string): string {
    return `/api/files/download?path=${encodeURIComponent(path)}`;
  },
  async history(path: string): Promise<{ entries: HistoryEntry[] }> {
    return requestJson(`/api/files/history?path=${encodeURIComponent(path)}`);
  },
  async rollback(path: string, commit: string): Promise<WriteResult> {
    return sendJson('POST', '/api/files/rollback', { path, commit });
  },
  async passkeyInfo(email: string): Promise<PasskeyInfo | null> {
    const res = await fetch(`/api/auth/passkey/info?email=${encodeURIComponent(email)}`, {
      credentials: 'same-origin',
    });
    if (res.status === 404) { return null; }
    return readJson(res);
  },
  async registerPasskey(payload: PasskeyInfo): Promise<void> {
    await sendJson('POST', '/api/auth/passkey/register', payload);
  },
  async deletePasskey(): Promise<void> {
    await requestJson('/api/auth/passkey', { method: 'DELETE' });
  },
  async move(from: string, to: string): Promise<{ path: string; is_directory: boolean }> {
    return sendJson('POST', '/api/files/move', { from, to });
  },
  async moveBulk(
    moves: { from: string; to: string }[],
  ): Promise<{ results: { path: string; is_directory: boolean }[] }> {
    return sendJson('POST', '/api/files/move/bulk', { moves });
  },
  async deleteFile(path: string): Promise<{ trash_path: string }> {
    return sendJson('DELETE', '/api/files/delete', { path });
  },
  async deleteFilesBulk(paths: string[]): Promise<{ results: { trash_path: string }[] }> {
    return sendJson('DELETE', '/api/files/delete/bulk', { paths });
  },
  async mkdir(path: string): Promise<void> {
    await sendJson('POST', '/api/files/mkdir', { path });
  },
  async meta(): Promise<{ meta: MetaMap }> {
    return requestJson('/api/files/meta');
  },
  async setTags(path: string, tags: string[]): Promise<void> {
    await sendJson('PUT', '/api/files/meta', { path, tags });
  },
  async setTagsBulk(updates: { path: string; tags: string[] }[]): Promise<void> {
    await sendJson('PUT', '/api/files/meta/bulk', { updates });
  },
  async agentStatus(): Promise<AgentStatus> {
    return requestJson('/api/agent/status');
  },
  async agentChat(messages: AgentMessage[]): Promise<AgentChatResponse> {
    return sendJson('POST', '/api/agent/chat', { messages });
  },
};

/** Upload via `XMLHttpRequest` so we can report progress, which `fetch` can't. */
function uploadWithProgress(
  body: FormData,
  onProgress: (loaded: number, total: number) => void,
): Promise<{ files: UploadResultItem[] }> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', '/api/files/upload', true);
    xhr.withCredentials = true;
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) { onProgress(event.loaded, event.total); }
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          resolve(JSON.parse(xhr.responseText));
        } catch (err) {
          reject(err);
        }
        return;
      }
      try {
        const parsed = JSON.parse(xhr.responseText);
        reject(
          new ApiError(
            xhr.status,
            parsed?.error?.code ?? 'unknown',
            parsed?.error?.message ?? xhr.statusText,
          ),
        );
      } catch {
        reject(new ApiError(xhr.status, 'unknown', xhr.statusText));
      }
    };
    xhr.onerror = () => reject(new ApiError(0, 'network', 'upload failed'));
    xhr.send(body);
  });
}

export interface MetaItem {
  tags: string[];
}

export type MetaMap = Record<string, MetaItem>;

export function firstMarkdownLeaf(tree: TreeNode[]): TreeFile | null {
  for (const node of tree) {
    if (node.type === 'file' && node.kind === 'md') { return node; }
    if (node.type === 'folder') {
      const hit = firstMarkdownLeaf(node.children);
      if (hit) { return hit; }
    }
  }
  return null;
}

export function flattenFiles(tree: TreeNode[]): TreeFile[] {
  const out: TreeFile[] = [];
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      if (node.type === 'file') { out.push(node); }
      else { walk(node.children); }
    }
  };
  walk(tree);
  return out;
}

export function topLevelFolders(tree: TreeNode[]): TreeFolder[] {
  return tree.filter((node): node is TreeFolder => node.type === 'folder');
}
