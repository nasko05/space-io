import { KeyboardEvent as ReactKeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import {
  AgentMessage,
  AgentPendingAction,
  AgentStatus,
  api,
  ApiError,
} from '../../api/client';
import { Close, Globe, Send, Sparkle } from '../icons/Icon';
import { str, toolLabel } from './format';
import styles from './AgentChat.module.css';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Called after a proposed change is applied, so the host can refresh the
   *  tree / excerpts / tags it shows elsewhere. */
  onVaultChanged: () => void;
}

const SUGGESTIONS = [
  'Summarise my most recent note.',
  'Find notes about a topic and list them.',
  'Organise my Journal folder by year.',
];

export function AgentChat({ open, onClose, onVaultChanged }: Props) {
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [pending, setPending] = useState<AgentPendingAction[]>([]);
  const [decisions, setDecisions] = useState<Record<string, string>>({});
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // Decisions accumulate synchronously here so rapid approvals across several
  // proposal cards can't race on the (async) `decisions` state. `inFlight`
  // guards against double-applying the same card before its result lands.
  const decisionsRef = useRef<Record<string, string>>({});
  const inFlightRef = useRef<Set<string>>(new Set());

  // Fetch agent availability the first time the drawer opens.
  useEffect(() => {
    if (!open || status) return;
    let cancelled = false;
    void (async () => {
      try {
        const s = await api.agentStatus();
        if (!cancelled) setStatus(s);
      } catch (err) {
        if (!cancelled) {
          setStatus({ configured: false, model: '', web_search: 'off' });
          console.error('agent status failed', err);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, status]);

  // Keep the transcript pinned to the latest message.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, pending, busy]);

  // Esc closes the drawer.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const runChat = useCallback(async (convo: AgentMessage[]) => {
    setBusy(true);
    setError(null);
    try {
      const resp = await api.agentChat(convo);
      setMessages(resp.messages);
      setPending(resp.pending_actions);
      decisionsRef.current = {};
      inFlightRef.current.clear();
      setDecisions({});
    } catch (err) {
      const msg =
        err instanceof ApiError ? err.message : err instanceof Error ? err.message : 'Request failed.';
      setError(msg);
    } finally {
      setBusy(false);
    }
  }, []);

  const send = useCallback(
    async (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || busy || pending.length > 0) return;
      const convo: AgentMessage[] = [...messages, { role: 'user', content: trimmed }];
      setMessages(convo);
      setInput('');
      await runChat(convo);
    },
    [busy, messages, pending.length, runChat],
  );

  // Apply one approved action through the existing, audited file endpoints.
  const applyAction = useCallback(
    async (action: AgentPendingAction): Promise<string> => {
      const a = action.args;
      switch (action.tool) {
        case 'write_file': {
          // Assistant edits are deliberate changes — record them as
          // checkpoints so each remains a recoverable point in history.
          const r = await api.checkpoint(
            str(a.path),
            str(a.content),
            a.message ? str(a.message) : undefined,
          );
          return `Wrote ${r.path}.`;
        }
        case 'move_path': {
          const r = await api.move(str(a.from), str(a.to));
          return `Moved to ${r.path}.`;
        }
        case 'delete_path': {
          const r = await api.deleteFile(str(a.path));
          return `Deleted; recoverable in trash at ${r.trash_path}.`;
        }
        case 'create_folder': {
          await api.mkdir(str(a.path));
          return `Created folder ${str(a.path)}.`;
        }
        case 'set_tags': {
          const tags = Array.isArray(a.tags) ? a.tags.map(str) : [];
          await api.setTags(str(a.path), tags);
          return `Updated tags on ${str(a.path)}.`;
        }
        default:
          return `Unknown action "${action.tool}".`;
      }
    },
    [],
  );

  // Once every pending action has a recorded result, append the tool results
  // (in the model's original order) and ask it to continue.
  const continueWith = useCallback(async () => {
    const resolved = decisionsRef.current;
    const toolMsgs: AgentMessage[] = pending.map((p) => ({
      role: 'tool',
      tool_call_id: p.tool_call_id,
      name: p.tool,
      content: resolved[p.tool_call_id] ?? 'User declined this change.',
    }));
    const convo = [...messages, ...toolMsgs];
    setPending([]);
    setMessages(convo);
    await runChat(convo);
  }, [messages, pending, runChat]);

  const decide = useCallback(
    async (action: AgentPendingAction, approve: boolean) => {
      const id = action.tool_call_id;
      if (busy || decisionsRef.current[id] !== undefined || inFlightRef.current.has(id)) return;
      inFlightRef.current.add(id);
      let result: string;
      if (approve) {
        try {
          result = await applyAction(action);
          onVaultChanged();
        } catch (err) {
          result = `Failed: ${err instanceof Error ? err.message : 'error'}`;
          setError(result);
        }
      } else {
        result = 'User declined this change.';
      }
      inFlightRef.current.delete(id);
      decisionsRef.current = { ...decisionsRef.current, [id]: result };
      setDecisions(decisionsRef.current);
      if (pending.every((p) => decisionsRef.current[p.tool_call_id] !== undefined)) {
        await continueWith();
      }
    },
    [applyAction, busy, continueWith, onVaultChanged, pending],
  );

  const rejectAll = useCallback(async () => {
    if (busy) return;
    for (const p of pending) {
      if (decisionsRef.current[p.tool_call_id] === undefined) {
        decisionsRef.current[p.tool_call_id] = 'User declined this change.';
      }
    }
    setDecisions({ ...decisionsRef.current });
    await continueWith();
  }, [busy, continueWith, pending]);

  const newChat = useCallback(() => {
    if (busy) return;
    decisionsRef.current = {};
    inFlightRef.current.clear();
    setMessages([]);
    setPending([]);
    setDecisions({});
    setError(null);
    setInput('');
  }, [busy]);

  const onInputKey = useCallback(
    (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        void send(input);
      }
    },
    [input, send],
  );

  if (!open) return null;

  const awaiting = pending.length > 0;
  const pendingIds = new Set(pending.map((p) => p.tool_call_id));
  const empty = messages.length === 0 && !busy;

  return (
    <aside className={styles.drawer} aria-label="AI assistant">
      <header className={styles.header}>
        <Sparkle size={18} />
        <div className={styles.headerText}>
          <div className={styles.title}>Assistant</div>
          {status && status.configured && (
            <div className={styles.subtitle}>
              {status.model}
              {status.web_search !== 'off' && (
                <span className={styles.webBadge} title={`web search: ${status.web_search}`}>
                  <Globe size={11} /> web
                </span>
              )}
            </div>
          )}
        </div>
        <button type="button" className={styles.iconBtn} onClick={newChat} title="New chat" disabled={busy}>
          <Sparkle size={15} />
        </button>
        <button type="button" className={styles.iconBtn} onClick={onClose} title="Close (Esc)">
          <Close size={16} />
        </button>
      </header>

      <div className={styles.body} ref={scrollRef}>
        {status && !status.configured && (
          <div className={styles.notice}>
            <strong>The assistant isn't configured.</strong>
            <p>
              Set <code>HEARTH_OPENROUTER_API_KEY</code> on the server (optionally{' '}
              <code>HEARTH_AGENT_MODEL</code> and <code>HEARTH_BRAVE_API_KEY</code>) and restart.
            </p>
          </div>
        )}

        {status?.configured && empty && (
          <div className={styles.intro}>
            <p className={styles.introLead}>
              I can read, search, write, move, and tidy your notes. Anything that changes a file is
              shown to you for approval first.
            </p>
            <div className={styles.suggestions}>
              {SUGGESTIONS.map((s) => (
                <button key={s} type="button" className={styles.suggestion} onClick={() => void send(s)}>
                  {s}
                </button>
              ))}
            </div>
          </div>
        )}

        {messages.map((m, i) => (
          <MessageView key={i} message={m} suppressIds={pendingIds} />
        ))}

        {busy && <div className={styles.thinking}>Thinking…</div>}

        {awaiting && (
          <div className={styles.proposals}>
            <div className={styles.proposalsLabel}>
              {pending.length === 1 ? 'Proposed change' : `${pending.length} proposed changes`}
            </div>
            {pending.map((p) => (
              <ProposalCard
                key={p.tool_call_id}
                action={p}
                decision={decisions[p.tool_call_id]}
                disabled={busy}
                onApprove={() => void decide(p, true)}
                onReject={() => void decide(p, false)}
              />
            ))}
            {pending.length > 1 && (
              <button type="button" className={styles.rejectAll} onClick={() => void rejectAll()} disabled={busy}>
                Reject all
              </button>
            )}
          </div>
        )}

        {error && <div className={styles.error}>{error}</div>}
      </div>

      <footer className={styles.footer}>
        <textarea
          className={styles.input}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onInputKey}
          placeholder={awaiting ? 'Resolve the proposed change above…' : 'Ask the assistant…'}
          rows={2}
          disabled={busy || awaiting || (status != null && !status.configured)}
          spellCheck
        />
        <button
          type="button"
          className={styles.sendBtn}
          onClick={() => void send(input)}
          disabled={busy || awaiting || !input.trim() || (status != null && !status.configured)}
          title="Send (Enter)"
        >
          <Send size={16} />
        </button>
      </footer>
    </aside>
  );
}

/** One transcript row: a user/assistant bubble plus muted activity lines for
 *  any read-only tool calls the assistant made. Pending (not-yet-decided)
 *  calls are suppressed here because they render as proposal cards instead. */
function MessageView({
  message,
  suppressIds,
}: {
  message: AgentMessage;
  suppressIds: Set<string>;
}) {
  if (message.role === 'tool') return null;

  if (message.role === 'user') {
    return <div className={styles.userBubble}>{message.content}</div>;
  }

  if (message.role !== 'assistant') return null;

  const calls = (message.tool_calls ?? []).filter((c) => !suppressIds.has(c.id));
  return (
    <>
      {message.content && <div className={styles.assistantBubble}>{message.content}</div>}
      {calls.map((c) => (
        <div key={c.id} className={styles.activity}>
          {toolLabel(c.function.name, c.function.arguments)}
        </div>
      ))}
    </>
  );
}



function ProposalCard({
  action,
  decision,
  disabled,
  onApprove,
  onReject,
}: {
  action: AgentPendingAction;
  decision: string | undefined;
  disabled: boolean;
  onApprove: () => void;
  onReject: () => void;
}) {
  const a = action.args;
  const decided = decision !== undefined;
  return (
    <div className={`${styles.card} ${decided ? styles.cardDecided : ''}`}>
      <div className={styles.cardSummary}>{action.summary}</div>

      {action.tool === 'write_file' && (
        <details className={styles.cardDetails}>
          <summary>Show content</summary>
          <pre className={styles.codeBlock}>{str(a.content)}</pre>
        </details>
      )}
      {action.tool === 'set_tags' && (
        <div className={styles.cardMeta}>{(Array.isArray(a.tags) ? a.tags : []).map(str).join(', ') || '(no tags)'}</div>
      )}

      {decided ? (
        <div className={styles.cardResult}>{decision}</div>
      ) : (
        <div className={styles.cardActions}>
          <button type="button" className={styles.approveBtn} onClick={onApprove} disabled={disabled}>
            Approve
          </button>
          <button type="button" className={styles.rejectBtn} onClick={onReject} disabled={disabled}>
            Reject
          </button>
        </div>
      )}
    </div>
  );
}

