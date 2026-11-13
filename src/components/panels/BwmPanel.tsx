import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface BwmBatteryStatus {
  voltage_mv: number;
  battery_percent: number;
  is_charging: boolean;
}

export interface BwmPanelProps {
  port: string;
  canAccessBwm: boolean;
  pollIntervalMs?: number;
}

function formatVoltage(voltageMv: number): string {
  return `${(voltageMv / 1000).toFixed(2)} V`;
}

export function BwmPanel({ port, canAccessBwm, pollIntervalMs = 5000 }: BwmPanelProps) {
  const [status, setStatus] = useState<BwmBatteryStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [pollingEnabled, setPollingEnabled] = useState<boolean>(true);
  const requestSeq = useRef(0);

  const fetchStatus = useCallback(
    async (manual = false) => {
      if (!canAccessBwm || !port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<BwmBatteryStatus>('get_bwm_battery_status', { port });
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
    [canAccessBwm, port],
  );

  useEffect(() => {
    requestSeq.current += 1; // invalidate in-flight responses
    setStatus(null);
    setError(null);
    setLoading(false);
    if (canAccessBwm && port) {
      void fetchStatus(false);
    }
  }, [canAccessBwm, port, fetchStatus]);

  useEffect(() => {
    if (!canAccessBwm || !pollingEnabled || !port) return;
    const id = window.setInterval(() => {
      void fetchStatus(false);
    }, pollIntervalMs);
    return () => window.clearInterval(id);
  }, [canAccessBwm, pollingEnabled, port, pollIntervalMs, fetchStatus]);

  if (!canAccessBwm) {
    return (
      <TerminalPanel title="BATTERY & WIRELESS">
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
          Battery &amp; Wireless Management requires PM5 capabilities v12 or higher. Feature disabled in current mode.
        </div>
      </TerminalPanel>
    );
  }

  const percent = status ? Math.max(0, Math.min(100, status.battery_percent)) : 0;

  return (
    <TerminalPanel title="BATTERY & WIRELESS">
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
            {loading ? 'Querying battery telemetry...' : 'No telemetry yet.'}
          </div>
        )}

        {status && (
          <div style={{ display: 'grid', gap: '8px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
              <span
                style={{
                  padding: '2px 8px',
                  border: '1px solid var(--green-dim)',
                  color: status.is_charging ? 'var(--amber)' : 'var(--green-bright)',
                  fontSize: '11px',
                  fontWeight: 600,
                }}
              >
                {status.is_charging ? 'CHARGING' : 'ON BATTERY'}
              </span>
              <span style={{ color: 'var(--green-bright)', fontSize: '13px', fontWeight: 600 }}>
                {percent}%
              </span>
              <span style={{ color: 'var(--green-dim)', fontSize: '12px' }}>
                {formatVoltage(status.voltage_mv)}
              </span>
            </div>

            <div>
              <div
                style={{
                  height: '14px',
                  border: '1px solid var(--green-dim)',
                  background: 'var(--bg-void)',
                  position: 'relative',
                  overflow: 'hidden',
                }}
                aria-label={`Battery ${percent}%`}
              >
                <div
                  style={{
                    width: `${percent}%`,
                    height: '100%',
                    background: status.is_charging ? 'var(--amber)' : 'var(--green-bright)',
                    boxShadow: status.is_charging
                      ? '0 0 8px rgba(255,184,0,0.35)'
                      : '0 0 8px rgba(0,255,65,0.25)',
                    transition: 'width 200ms linear',
                  }}
                />
              </div>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  color: 'var(--green-dim)',
                  fontSize: '10px',
                  marginTop: '2px',
                }}
              >
                <span>0%</span>
                <span>50%</span>
                <span>100%</span>
              </div>
            </div>

            <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
              <span style={{ color: 'var(--green-dim)' }}>VOLTAGE</span>
              <span>{formatVoltage(status.voltage_mv)} ({status.voltage_mv} mV)</span>
              <span style={{ color: 'var(--green-dim)' }}>LEVEL</span>
              <span>{percent}%</span>
              <span style={{ color: 'var(--green-dim)' }}>STATE</span>
              <span style={{ color: status.is_charging ? 'var(--amber)' : 'var(--green-bright)' }}>
                {status.is_charging ? 'Charging' : 'On Battery'}
              </span>
            </div>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
