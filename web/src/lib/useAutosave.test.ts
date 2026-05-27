import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useAutosave, saveStatusLabel } from './useAutosave';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useAutosave', () => {
  it('starts in idle status', () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useAutosave({ onSave }));
    expect(result.current.status.kind).toBe('idle');
  });

  it('transitions to dirty then saved after delay', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useAutosave({ onSave, delayMs: 500 }));

    act(() => {
      result.current.markDirty('hello');
    });
    expect(result.current.status.kind).toBe('dirty');

    await act(async () => {
      vi.advanceTimersByTime(500);
    });

    expect(onSave).toHaveBeenCalledWith('hello');
    expect(result.current.status.kind).toBe('saved');
  });

  it('flush() saves immediately without waiting for delay', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useAutosave({ onSave, delayMs: 5000 }));

    act(() => {
      result.current.markDirty('urgent');
    });

    await act(async () => {
      await result.current.flush();
    });

    expect(onSave).toHaveBeenCalledWith('urgent');
    expect(result.current.status.kind).toBe('saved');
  });

  it('only saves the latest content if edited multiple times', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useAutosave({ onSave, delayMs: 300 }));

    act(() => {
      result.current.markDirty('first');
    });
    act(() => {
      result.current.markDirty('second');
    });
    act(() => {
      result.current.markDirty('third');
    });

    await act(async () => {
      vi.advanceTimersByTime(300);
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith('third');
  });

  it('reports error status on save failure', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('network'));
    const { result } = renderHook(() => useAutosave({ onSave, delayMs: 100 }));

    act(() => {
      result.current.markDirty('fail');
    });

    await act(async () => {
      vi.advanceTimersByTime(100);
    });

    expect(result.current.status.kind).toBe('error');
    if (result.current.status.kind === 'error') {
      expect(result.current.status.message).toBe('network');
    }
  });

  it('resets to idle when onSave identity changes', () => {
    const onSave1 = vi.fn().mockResolvedValue(undefined);
    const onSave2 = vi.fn().mockResolvedValue(undefined);
    const { result, rerender } = renderHook(
      ({ onSave }) => useAutosave({ onSave }),
      { initialProps: { onSave: onSave1 } },
    );

    act(() => {
      result.current.markDirty('hello');
    });
    expect(result.current.status.kind).toBe('dirty');

    rerender({ onSave: onSave2 });
    expect(result.current.status.kind).toBe('idle');
  });
});

describe('saveStatusLabel', () => {
  it('returns empty for idle', () => {
    expect(saveStatusLabel({ kind: 'idle' })).toBe('');
  });

  it('returns "unsaved" for dirty', () => {
    expect(saveStatusLabel({ kind: 'dirty' })).toBe('unsaved');
  });

  it('returns "saving…" for saving', () => {
    expect(saveStatusLabel({ kind: 'saving' })).toBe('saving…');
  });

  it('returns a label with seconds for saved', () => {
    const label = saveStatusLabel({ kind: 'saved', at: Date.now() - 5000 });
    expect(label).toMatch(/auto-saved \d+s ago/);
  });

  it('returns error message for error', () => {
    expect(saveStatusLabel({ kind: 'error', message: 'fail' })).toBe('save failed: fail');
  });
});
