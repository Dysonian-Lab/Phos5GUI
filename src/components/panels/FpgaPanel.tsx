import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface FpgaStatus {
  bitstream_loaded: boolean;
  fpga_version: string;
  clock_speed_hz: number;
  pwm_low_count: number;
}

export interface FpgaPanelProps {
  port: string;
  canAccessFpga: boolean;
  pollIntervalMs?: number;
}

function formatClockSpeed(hz: number): string {
  if (hz >= 1_000_000) {
    return `${(hz / 1_000_000).toFixed(2)} MHz`;
  }
  if (hz >= 1_000) {
    return `${(hz / 1_000).toFixed(2)} kHz`;
  }
  return `${hz} Hz`;
}

export function FpgaPanel({ port, canAccessFpga, pollIntervalMs = 5000 }: FpgaPanelProps) {
  const [status, setStatus] = useState<FpgaStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [pollingEnabled, setPollingEnabled] = useState<boolean>(true);
  const requestSeq = useRef(0);

  const fetchStatus = useCallback(
    async (manual = false) => {
      if (!canAccessFpga || !port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<FpgaStatus>('get_fpga_status', { port });
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
    [canAccessFpga, port],
  );

  useEffect(() => {
    requestSeq.current += 1;
    setStatus(null);
    setError(null);
    setLoading(false);
    if (canAccessFpga && port) {
      void fetchStatus(false);
    }
  }, [canAccessFpga, port, fetchStatus]);

  useEffect(() => {
    if (!canAccessFpga || !pollingEnabled || !port) return;
    const id = window.setInterval(() => {
      void fetchStatus(false);
    }, pollIntervalMs);
    return () => window.clearInterval(id);
  }, [canAccessFpga, pollingEnabled, port, pollIntervalMs, fetchStatus]);

  if (!canAccessFpga) {
    return (
      <TerminalPanel title="FPGA MANAGEMENT">
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
          FPGA Management unavailable in current mode.
        </div>
      </TerminalPanel>
    );
  }

  const bitstreamLoaded = status?.bitstream_loaded ?? false;
  const clockMhz = status ? formatClockSpeed(status.clock_speed_hz) : 'N/A';

  return (
    <TerminalPanel title="FPGA MANAGEMENT">
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
            {loading ? 'Querying FPGA status...' : 'No status yet.'}
          </div>
        )}

        {status && (
          <div style={{ display: 'grid', gap: '10px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
              <span
                style={{
                  padding: '2px 8px',
                  border: `1px solid ${bitstreamLoaded ? 'var(--green-bright)' : 'var(--amber)'}`,
                  color: bitstreamLoaded ? 'var(--green-bright)' : 'var(--amber)',
                  fontSize: '11px',
                  fontWeight: 600,
                  background: 'var(--bg-void)',
                }}
              >
                {bitstreamLoaded ? 'LOADED' : 'NOT LOADED'}
              </span>
              <span style={{ color: 'var(--green-dim)', fontSize: '12px' }}>
                {status.fpga_version || 'None / Unspecified'}
              </span>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
              <span style={{ color: 'var(--green-dim)' }}>BITSTREAM</span>
              <span style={{ color: bitstreamLoaded ? 'var(--green-bright)' : 'var(--amber)' }}>
                {bitstreamLoaded ? 'LOADED' : 'NOT LOADED'}
              </span>
              <span style={{ color: 'var(--green-dim)' }}>VERSION</span>
              <span>{status.fpga_version || 'None / Unspecified'}</span>
              <span style={{ color: 'var(--green-dim)' }}>CLOCK</span>
              <span>{clockMhz}</span>
              <span style={{ color: 'var(--green-dim)' }}>PWM LOW COUNT</span>
              <span>{status.pwm_low_count}</span>
            </div>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
