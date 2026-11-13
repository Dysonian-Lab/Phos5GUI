import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface CepStatus {
  connected: boolean;
  baudrate: number;
  rx_count: number;
  tx_count: number;
}

export interface CepPanelProps {
  port: string;
  canAccessCep: boolean;
  pollIntervalMs?: number;
}

export function CepPanel({ port, canAccessCep, pollIntervalMs = 5000 }: CepPanelProps) {
  const [status, setStatus] = useState<CepStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [pollingEnabled, setPollingEnabled] = useState<boolean>(true);
  const requestSeq = useRef(0);

  const fetchStatus = useCallback(
    async (manual = false) => {
      if (!canAccessCep || !port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<CepStatus>('get_cep_status', { port });
        if (seq !== requestSeq.current) return;
        setStatus(result);
      } catch (err) {
        if (seq !== requestSeq.current) return;
        const message =
          typeof err === 'string'
            ? err
            : err instanceof Error
              ? err.message
              : JSON.stringify(err);
        setError(message);
      } finally {
        if (seq === requestSeq.current) setLoading(false);
      }
    },
    [canAccessCep, port],
  );

  useEffect(() => {
    requestSeq.current += 1;
    setStatus(null);
    setError(null);
    setLoading(false);
    if (canAccessCep && port) {
      void fetchStatus(false);
    }
  }, [canAccessCep, port, fetchStatus]);

  useEffect(() => {
    if (!canAccessCep || !pollingEnabled || !port) return;
    const id = window.setInterval(() => {
      void fetchStatus(false);
    }, pollIntervalMs);
    return () => window.clearInterval(id);
  }, [canAccessCep, pollingEnabled, port, pollIntervalMs, fetchStatus]);

  if (!canAccessCep) {
    return (
      <TerminalPanel title="FLIPPER ZERO (CEP)">
        <div
          style={{
            fontFamily: 'var(--font-mono)',
            fontSize: '13px',
            color: 'var(--amber)',
            border: '1px solid var(--amber)',
            padding: '10px 12px',
            background: 'var(--bg-panel)',
            opacity: 0.85,
          }}
        >
          Flipper Zero CEP interface requires PM5 capabilities v13 or higher. Feature disabled in current mode.
        </div>
      </TerminalPanel>
    );
  }

  return (
    <TerminalPanel title="FLIPPER ZERO (CEP)">
      <div style={{ fontFamily: 'var(--font-mono)', fontSize: '13px', color: 'var(--green-bright)' }}>
        <div style={{ display: 'flex', gap: '8px', marginBottom: '10px', flexWrap: 'wrap', alignItems: 'center' }}>
          <button
            type="button"
            onClick={() => void fetchStatus(true)}
            disabled={loading}
            style={{
              background: 'var(--bg-void)',
              color: 'var(--green-bright)',
              border: '1px solid var(--green-dim)',
              fontFamily: 'var(--font-mono)',
              fontSize: '12px',
              padding: '4px 10px',
              cursor: loading ? 'not-allowed' : 'pointer',
              opacity: loading ? 0.6 : 1,
            }}
          >
            {loading ? 'REFRESHING...' : 'REFRESH'}
          </button>
          <button
            type="button"
            onClick={() => setPollingEnabled((v) => !v)}
            style={{
              background: 'var(--bg-void)',
              color: pollingEnabled ? 'var(--green-bright)' : 'var(--amber)',
              border: '1px solid var(--green-dim)',
              fontFamily: 'var(--font-mono)',
              fontSize: '12px',
              padding: '4px 10px',
              cursor: 'pointer',
            }}
          >
            POLL: {pollingEnabled ? 'ON' : 'OFF'}
          </button>
          <span style={{ color: 'var(--green-dim)', fontSize: '11px' }}>
            {port ? `PORT ${port}` : 'NO PORT'} · {pollIntervalMs / 1000}s
          </span>
        </div>

        {error && (
          <div
            style={{
              marginBottom: '10px',
              color: 'var(--amber)',
              border: '1px solid var(--amber)',
              padding: '6px 8px',
              background: 'var(--bg-panel)',
              fontSize: '12px',
            }}
          >
            [!] {error}
          </div>
        )}

        {!status && !error && (
          <div style={{ color: 'var(--green-dim)', fontSize: '12px' }}>
            {loading ? 'Querying CEP status...' : 'No status yet.'}
          </div>
        )}

        {status && (
          <div style={{ display: 'grid', gap: '10px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
              <span
                style={{
                  padding: '2px 8px',
                  border: '1px solid var(--green-dim)',
                  color: status.connected ? 'var(--green-bright)' : 'var(--amber)',
                  fontSize: '11px',
                  fontWeight: 600,
                  background: 'var(--bg-void)',
                }}
              >
                {status.connected ? 'CONNECTED' : 'DISCONNECTED'}
              </span>
              <span style={{ color: 'var(--green-dim)', fontSize: '12px' }}>
                {status.baudrate} bps
              </span>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
              <span style={{ color: 'var(--green-dim)' }}>LINK</span>
              <span style={{ color: status.connected ? 'var(--green-bright)' : 'var(--amber)' }}>
                {status.connected ? 'CONNECTED' : 'DISCONNECTED'}
              </span>
              <span style={{ color: 'var(--green-dim)' }}>BAUDRATE</span>
              <span>{status.baudrate} bps</span>
              <span style={{ color: 'var(--green-dim)' }}>RX COUNT</span>
              <span>{status.rx_count}</span>
              <span style={{ color: 'var(--green-dim)' }}>TX COUNT</span>
              <span>{status.tx_count}</span>
            </div>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
