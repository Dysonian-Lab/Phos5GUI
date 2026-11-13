import { useCallback, useEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type PipelineStatusString =
  | 'idle'
  | 'fetching'
  | 'update_available'
  | 'merging'
  | 'patching'
  | 'compiling_firmware'
  | 'compiling_client'
  | 'hot_swapping'
  | 'flashing'
  | 'complete'
  | 'failed';

export interface UpdateCheckResult {
  updates_available: boolean;
  current_commit: string;
  latest_commit: string;
  commit_logs: string[];
}

export interface UpdateProgressPayload {
  status: PipelineStatusString;
  stage: string;
  percent: number;
  message: string;
  output?: string;
  stream?: string;
  failed_commit?: string;
}

interface LogLine {
  stream: string;
  text: string;
}

export interface Pm5UpdateModalProps {
  isOpen: boolean;
  port: string;
  onClose: () => void;
}

const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0, 0, 0, 0.85)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
};

const panelStyle: CSSProperties = {
  background: 'var(--bg-panel, #0A0A0A)',
  border: '2px solid var(--green-bright, #00FF41)',
  padding: '20px 24px',
  width: 'min(720px, calc(100vw - 48px))',
  maxHeight: 'calc(100vh - 64px)',
  overflow: 'auto',
  fontFamily: 'var(--font-mono)',
  color: 'var(--green-bright, #00FF41)',
};

const titleStyle: CSSProperties = {
  color: 'var(--green-bright, #00FF41)',
  fontSize: '15px',
  fontWeight: 700,
  marginBottom: '12px',
  textTransform: 'uppercase',
};

const bodyStyle: CSSProperties = {
  fontSize: '13px',
  lineHeight: 1.7,
  color: 'var(--green-mid, #00CC33)',
};

const primaryBtn: CSSProperties = {
  background: 'var(--bg-void, #000)',
  color: 'var(--green-bright, #00FF41)',
  border: '2px solid var(--green-bright, #00FF41)',
  fontFamily: 'var(--font-mono)',
  fontSize: '13px',
  fontWeight: 600,
  padding: '8px 18px',
  cursor: 'pointer',
  textTransform: 'uppercase',
};

const secondaryBtn: CSSProperties = {
  ...primaryBtn,
  color: 'var(--amber, #FFB800)',
  border: '2px solid var(--amber, #FFB800)',
};

const disabledStyle: CSSProperties = {
  opacity: 0.5,
  cursor: 'not-allowed',
};

const STAGES: { label: string; statuses: PipelineStatusString[] }[] = [
  { label: '[1/4] Pulling code', statuses: ['fetching', 'update_available', 'merging', 'patching'] },
  { label: '[2/4] Compiling Firmware (-j4)', statuses: ['compiling_firmware'] },
  { label: '[3/4] Building Client', statuses: ['compiling_client'] },
  { label: '[4/4] Flashing Board', statuses: ['hot_swapping', 'flashing'] },
];

const MAX_LOG_LINES = 4000;

function shortCommit(commit: string | undefined): string {
  if (!commit) return '--------';
  return commit.trim().slice(0, 7);
}

function activeStageIndex(status: PipelineStatusString | null): number {
  if (!status) return -1;
  if (status === 'complete') return STAGES.length;
  return STAGES.findIndex((s) => s.statuses.includes(status));
}

export function Pm5UpdateModal({ isOpen, port, onClose }: Pm5UpdateModalProps) {
  const [checkResult, setCheckResult] = useState<UpdateCheckResult | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);

  const [isUpdating, setIsUpdating] = useState(false);
  const [progress, setProgress] = useState<UpdateProgressPayload | null>(null);
  const [updateResult, setUpdateResult] = useState<string | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const [logLines, setLogLines] = useState<LogLine[]>([]);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const logEndRef = useRef<HTMLDivElement | null>(null);

  const runCheck = useCallback(async () => {
    setChecking(true);
    setCheckError(null);
    setUpdateError(null);
    setUpdateResult(null);
    try {
      const result = await invoke<UpdateCheckResult>('check_pm5_updates');
      setCheckResult(result);
    } catch (e) {
      setCheckResult(null);
      setCheckError(typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e));
    } finally {
      setChecking(false);
    }
  }, []);

  // Run the check each time the modal opens.
  useEffect(() => {
    if (isOpen) {
      void runCheck();
    }
  }, [isOpen, runCheck]);

  // Subscribe to pipeline progress for the lifetime of the modal.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    (async () => {
      unlisten = await listen<UpdateProgressPayload>('pm5-update-progress', (event) => {
        const p = event.payload;
        setProgress(p);
        if (p.output) {
          const line: LogLine = { stream: p.stream ?? 'stdout', text: p.output };
          setLogLines((prev) => {
            const next = prev.length >= MAX_LOG_LINES ? prev.slice(prev.length - MAX_LOG_LINES + 1) : prev.slice();
            next.push(line);
            return next;
          });
          setDrawerOpen(true);
        }
      });
      if (cancelled && unlisten) {
        unlisten();
        unlisten = null;
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  // Auto-scroll the terminal drawer to the newest line.
  useEffect(() => {
    if (drawerOpen) {
      logEndRef.current?.scrollIntoView({ block: 'end' });
    }
  }, [logLines, drawerOpen]);

  const startUpdate = async () => {
    if (!port) {
      setUpdateError('No device port detected. Connect a PM5 device first.');
      return;
    }
    setIsUpdating(true);
    setUpdateError(null);
    setUpdateResult(null);
    setProgress(null);
    setLogLines([]);
    try {
      const res = await invoke<string>('execute_pm5_update', { port });
      setUpdateResult(res);
      setProgress({
        status: 'complete',
        stage: '[4/4] Flashing Board',
        percent: 100,
        message: res,
      });
    } catch (e) {
      setUpdateError(typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e));
    } finally {
      setIsUpdating(false);
    }
  };

  if (!isOpen) return null;

  const percent = Math.max(0, Math.min(100, progress?.percent ?? 0));
  const activeIdx = activeStageIndex(progress?.status ?? null);
  const isFailed = progress?.status === 'failed' || !!updateError;
  const isComplete = progress?.status === 'complete' && !updateError;

  return (
    <div style={overlayStyle} onClick={isUpdating ? undefined : onClose} role="dialog" aria-modal="true">
      <div style={panelStyle} onClick={(e) => e.stopPropagation()}>
        <div style={titleStyle}>PM5 Upstream Update &amp; Flash</div>

        {/* Check status */}
        <div style={{ ...bodyStyle, marginBottom: '12px' }}>
          {checking && <span>Checking for updates...</span>}
          {!checking && checkError && (
            <span style={{ color: 'var(--red-bright, #FF0033)' }}>[!!] {checkError}</span>
          )}
          {!checking && !checkError && checkResult && !checkResult.updates_available && (
            <span>
              Firmware and client are up to date.
              <span style={{ color: 'var(--green-dim)' }}> (HEAD {shortCommit(checkResult.current_commit)})</span>
            </span>
          )}
          {!checking && !checkError && checkResult && checkResult.updates_available && (
            <span>
              {checkResult.commit_logs.length} new commit(s) available:
              <span style={{ color: 'var(--green-dim)' }}>
                {' '}
                {shortCommit(checkResult.current_commit)} -&gt; {shortCommit(checkResult.latest_commit)}
              </span>
            </span>
          )}
        </div>

        {/* Changelog */}
        {!checking && !checkError && checkResult?.updates_available && (
          <div
            style={{
              border: '1px solid var(--green-dim)',
              background: 'var(--bg-void)',
              padding: '8px 10px',
              maxHeight: '140px',
              overflow: 'auto',
              fontSize: '12px',
              lineHeight: 1.6,
              marginBottom: '14px',
            }}
          >
            {checkResult.commit_logs.map((line, idx) => (
              <div key={`${idx}-${line}`} style={{ color: 'var(--green-mid)' }}>
                {line}
              </div>
            ))}
          </div>
        )}

        {/* Progress */}
        {(isUpdating || progress) && (
          <div style={{ marginBottom: '14px' }}>
            <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap', marginBottom: '8px' }}>
              {STAGES.map((stage, idx) => {
                const done = activeIdx > idx || isComplete;
                const active = activeIdx === idx && !isComplete;
                const color = done
                  ? 'var(--green-bright)'
                  : active
                    ? 'var(--amber, #FFB800)'
                    : 'var(--green-dim)';
                return (
                  <span
                    key={stage.label}
                    style={{
                      fontSize: '10px',
                      padding: '2px 6px',
                      border: `1px solid ${color}`,
                      color,
                      opacity: done || active ? 1 : 0.5,
                    }}
                  >
                    {stage.label}
                  </span>
                );
              })}
            </div>
            <div style={{ fontSize: '12px', color: 'var(--green-mid)', marginBottom: '6px' }}>
              {progress?.message ?? '[1/4] Pulling code'}
            </div>
            <div style={{ height: '10px', border: '1px solid var(--green-dim)', background: 'var(--bg-void)' }}>
              <div
                style={{
                  height: '100%',
                  width: `${percent}%`,
                  background: isFailed ? 'var(--red-bright, #FF0033)' : 'var(--green-bright)',
                  transition: 'width 150ms linear',
                }}
              />
            </div>
            <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginTop: '4px' }}>
              {percent}% {progress?.status ? `· ${progress.status}` : ''}
            </div>
          </div>
        )}

        {updateResult && (
          <div style={{ marginBottom: '10px', color: 'var(--green-bright)', fontSize: '12px', whiteSpace: 'pre-wrap' }}>
            [+] {updateResult}
          </div>
        )}
        {updateError && (
          <div style={{ marginBottom: '10px', color: 'var(--red-bright, #FF0033)', fontSize: '12px', whiteSpace: 'pre-wrap' }}>
            [!!] {updateError}
          </div>
        )}

        {/* Actions */}
        <div style={{ display: 'flex', gap: '12px', flexWrap: 'wrap', alignItems: 'center' }}>
          <button
            type="button"
            style={{
              ...primaryBtn,
              ...((!checkResult?.updates_available || isUpdating || checking) ? disabledStyle : {}),
            }}
            onClick={() => void startUpdate()}
            disabled={!checkResult?.updates_available || isUpdating || checking}
          >
            {isUpdating ? 'Updating & Flashing...' : '1-Click Update & Flash'}
          </button>
          <button
            type="button"
            style={secondaryBtn}
            onClick={() => setDrawerOpen((v) => !v)}
            disabled={logLines.length === 0}
          >
            {drawerOpen ? 'Hide Build Log' : `Build Log (${logLines.length})`}
          </button>
          <button type="button" style={secondaryBtn} onClick={onClose} disabled={isUpdating}>
            Close
          </button>
          <span style={{ color: 'var(--green-dim)', fontSize: '11px' }}>PORT: {port || '---'}</span>
        </div>

        {/* Terminal drawer */}
        {drawerOpen && (
          <div
            style={{
              marginTop: '14px',
              border: '1px solid var(--green-dim)',
              background: 'var(--bg-void)',
              height: '220px',
              overflow: 'auto',
              padding: '8px 10px',
              fontSize: '11px',
              lineHeight: 1.5,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
            }}
          >
            {logLines.length === 0 && <span style={{ color: 'var(--green-dim)' }}>No build output yet.</span>}
            {logLines.map((line, idx) => (
              <div
                key={`${idx}-${line.text}`}
                style={{ color: line.stream === 'stderr' ? 'var(--red-bright, #FF0033)' : 'var(--green-mid)' }}
              >
                {line.text}
              </div>
            ))}
            <div ref={logEndRef} />
          </div>
        )}
      </div>
    </div>
  );
}