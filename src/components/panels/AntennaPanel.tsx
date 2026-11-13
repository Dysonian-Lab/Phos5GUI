import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface AntennaStatus {
  lf_frequency_hz: number;
  lf_voltage_mv: number;
  hf_voltage_mv: number;
  tuning_active: boolean;
}

export interface AntennaPanelProps {
  port: string;
  pollIntervalMs?: number;
}

function formatVoltage(mv: number): string {
  const v = mv / 1000;
  return `${v.toFixed(2)} V (${mv} mV)`;
}

export function AntennaPanel({ port, pollIntervalMs = 5000 }: AntennaPanelProps) {
  const [status, setStatus] = useState<AntennaStatus | null>(null);
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
        const result = await invoke<AntennaStatus>('get_antenna_status', { port });
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

  const tuningActive = status?.tuning_active ?? false;
  const lfKhz = status ? status.lf_frequency_hz / 1000 : 0;

  return (
    <TerminalPanel title="ANTENNA TELEMETRY">
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
            {loading ? 'Querying antenna status...' : 'No status yet.'}
          </div>
        )}

        {status && (
          <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
            <span style={{ color: 'var(--green-dim)' }}>LF FREQUENCY</span>
            <span style={{ color: 'var(--green-bright)' }}>{lfKhz.toFixed(2)} kHz</span>
            <span style={{ color: 'var(--green-dim)' }}>LF VOLTAGE</span>
            <span style={{ color: 'var(--green-bright)' }}>{formatVoltage(status.lf_voltage_mv)}</span>
            <span style={{ color: 'var(--green-dim)' }}>HF VOLTAGE</span>
            <span style={{ color: 'var(--green-bright)' }}>{formatVoltage(status.hf_voltage_mv)}</span>
            <span style={{ color: 'var(--green-dim)' }}>TUNING STATUS</span>
            <span
              style={{
                padding: '2px 8px',
                border: `1px solid ${tuningActive ? 'var(--green-bright)' : 'var(--amber)'}`,
                color: tuningActive ? 'var(--green-bright)' : 'var(--amber)',
                fontSize: '11px',
                fontWeight: 600,
                background: 'var(--bg-void)',
              }}
            >
              {tuningActive ? 'ACTIVE' : 'IDLE'}
            </span>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
