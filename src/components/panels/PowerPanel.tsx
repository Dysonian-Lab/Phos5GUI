import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface PowerStatus {
  v33_enabled: boolean;
  v50_enabled: boolean;
  v33_voltage_mv: number;
  v50_voltage_mv: number;
}

export interface PowerPanelProps {
  port: string;
  pollIntervalMs?: number;
}

function formatVoltage(mv: number): string {
  return `${(mv / 1000).toFixed(2)} V`;
}

export function PowerPanel({ port, pollIntervalMs = 5000 }: PowerPanelProps) {
  const [status, setStatus] = useState<PowerStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [pollingEnabled, setPollingEnabled] = useState<boolean>(true);
  const requestSeq = useRef(0);

  const fetchStatus = useCallback(
    async (manual = false) => {
      if (!port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<PowerStatus>('get_power_status', { port });
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
    [port],
  );

  useEffect(() => {
    requestSeq.current += 1;
    setStatus(null);
    setError(null);
    setLoading(false);
    if (port) {
      void fetchStatus(false);
    }
  }, [port, fetchStatus]);

  useEffect(() => {
    if (!pollingEnabled || !port) return;
    const id = window.setInterval(() => {
      void fetchStatus(false);
    }, pollIntervalMs);
    return () => window.clearInterval(id);
  }, [pollingEnabled, port, pollIntervalMs, fetchStatus]);

  const setRail = async (rail: '3v3' | '5v0', enabled: boolean) => {
    setError(null);
    try {
      await invoke('set_power_rail', { port, rail, enabled });
      void fetchStatus(false);
    } catch (err) {
      const message =
        typeof err === 'string'
          ? err
          : err instanceof Error
            ? err.message
            : JSON.stringify(err);
      setError(message);
    }
  };

  const railStatus = (enabled: boolean) => (enabled ? 'ACTIVE' : 'INACTIVE');

  return (
    <TerminalPanel title="POWER RAILS">
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
            {loading ? 'Querying power status...' : 'No status yet.'}
          </div>
        )}

        {status && (
          <div style={{ marginTop: '8px' }}>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '10px',
                marginBottom: '10px',
                flexWrap: 'wrap',
              }}
            >
              <span style={{ color: 'var(--green-dim)' }}>3.3V RAIL</span>
              <span
                style={{
                  padding: '2px 8px',
                  border: `1px solid ${status.v33_enabled ? 'var(--green-bright)' : 'var(--amber)'}`,
                  color: status.v33_enabled ? 'var(--green-bright)' : 'var(--amber)',
                  fontSize: '11px',
                  fontWeight: 600,
                  background: 'var(--bg-void)',
                }}
              >
                {railStatus(status.v33_enabled)}
              </span>
              <span style={{ color: 'var(--green-dim)' }}>{formatVoltage(status.v33_voltage_mv)}</span>
              <button
                type="button"
                onClick={() => void setRail('3v3', !status.v33_enabled)}
                style={{
                  background: 'var(--bg-void)',
                  color: 'var(--green-bright)',
                  border: '1px solid var(--green-dim)',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '11px',
                  padding: '2px 8px',
                  cursor: 'pointer',
                }}
              >
                {status.v33_enabled ? 'DISABLE' : 'ENABLE'}
              </button>
            </div>

            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '10px',
                marginBottom: '10px',
                flexWrap: 'wrap',
              }}
            >
              <span style={{ color: 'var(--green-dim)' }}>5.0V RAIL</span>
              <span
                style={{
                  padding: '2px 8px',
                  border: `1px solid ${status.v50_enabled ? 'var(--green-bright)' : 'var(--amber)'}`,
                  color: status.v50_enabled ? 'var(--green-bright)' : 'var(--amber)',
                  fontSize: '11px',
                  fontWeight: 600,
                  background: 'var(--bg-void)',
                }}
              >
                {railStatus(status.v50_enabled)}
              </span>
              <span style={{ color: 'var(--green-dim)' }}>{formatVoltage(status.v50_voltage_mv)}</span>
              <button
                type="button"
                onClick={() => void setRail('5v0', !status.v50_enabled)}
                style={{
                  background: 'var(--bg-void)',
                  color: 'var(--green-bright)',
                  border: '1px solid var(--green-dim)',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '11px',
                  padding: '2px 8px',
                  cursor: 'pointer',
                }}
              >
                {status.v50_enabled ? 'DISABLE' : 'ENABLE'}
              </button>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
              <span style={{ color: 'var(--green-dim)' }}>3.3V VOLTAGE</span>
              <span>{formatVoltage(status.v33_voltage_mv)}</span>
              <span style={{ color: 'var(--green-dim)' }}>5.0V VOLTAGE</span>
              <span>{formatVoltage(status.v50_voltage_mv)}</span>
            </div>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
