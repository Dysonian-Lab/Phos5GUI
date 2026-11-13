import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface RgbStatus {
  red: number;
  green: number;
  blue: number;
  mode: number;
  brightness: number;
}

const MODE_LABELS: Record<number, string> = {
  0: 'OFF',
  1: 'SOLID',
  2: 'BREATHING',
  3: 'RAINBOW',
};

export interface RgbPanelProps {
  port: string;
  pollIntervalMs?: number;
}

export function RgbPanel({ port, pollIntervalMs = 5000 }: RgbPanelProps) {
  const [status, setStatus] = useState<RgbStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [pollingEnabled, setPollingEnabled] = useState<boolean>(true);
  const [localRed, setLocalRed] = useState<number>(0);
  const [localGreen, setLocalGreen] = useState<number>(0);
  const [localBlue, setLocalBlue] = useState<number>(0);
  const [localMode, setLocalMode] = useState<number>(1);
  const [localBrightness, setLocalBrightness] = useState<number>(100);
  const requestSeq = useRef(0);

  const fetchStatus = useCallback(
    async (manual = false) => {
      if (!port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<RgbStatus>('get_rgb_status', { port });
        if (seq !== requestSeq.current) return;
        setStatus(result);
        setLocalRed(result.red);
        setLocalGreen(result.green);
        setLocalBlue(result.blue);
        setLocalMode(result.mode);
        setLocalBrightness(result.brightness);
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

  const applyConfig = async () => {
    setError(null);
    try {
      await invoke('set_rgb_config', {
        port,
        red: localRed,
        green: localGreen,
        blue: localBlue,
        mode: localMode,
        brightness: localBrightness,
      });
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

  const active = status ?? { red: localRed, green: localGreen, blue: localBlue, mode: localMode, brightness: localBrightness };

  return (
    <TerminalPanel title="RGB LED CONTROL">
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
            {loading ? 'Querying RGB status...' : 'No status yet.'}
          </div>
        )}

        <div style={{ marginTop: '12px', paddingTop: '8px', borderTop: '1px solid var(--green-dim)' }}>
          <div style={{ color: 'var(--green-dim)', fontSize: '11px', marginBottom: '8px' }}>
            CONFIGURE RGB LED
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '8px', alignItems: 'end' }}>
            <div>
              <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginBottom: '4px' }}>RED</div>
              <input
                type="range"
                min="0"
                max="255"
                value={localRed}
                onChange={(e) => setLocalRed(Number(e.target.value))}
                style={{ width: '100%', accentColor: 'var(--green-bright)' }}
              />
              <div style={{ fontSize: '11px', color: 'var(--green-bright)', marginTop: '2px' }}>{localRed}</div>
            </div>
            <div>
              <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginBottom: '4px' }}>GREEN</div>
              <input
                type="range"
                min="0"
                max="255"
                value={localGreen}
                onChange={(e) => setLocalGreen(Number(e.target.value))}
                style={{ width: '100%', accentColor: 'var(--green-bright)' }}
              />
              <div style={{ fontSize: '11px', color: 'var(--green-bright)', marginTop: '2px' }}>{localGreen}</div>
            </div>
            <div>
              <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginBottom: '4px' }}>BLUE</div>
              <input
                type="range"
                min="0"
                max="255"
                value={localBlue}
                onChange={(e) => setLocalBlue(Number(e.target.value))}
                style={{ width: '100%', accentColor: 'var(--green-bright)' }}
              />
              <div style={{ fontSize: '11px', color: 'var(--green-bright)', marginTop: '2px' }}>{localBlue}</div>
            </div>
            <div>
              <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginBottom: '4px' }}>MODE</div>
              <select
                value={localMode}
                onChange={(e) => setLocalMode(Number(e.target.value))}
                style={{
                  width: '100%',
                  fontFamily: 'var(--font-mono)',
                  fontSize: '12px',
                  background: 'var(--bg-void)',
                  color: 'var(--green-bright)',
                  border: '1px solid var(--green-dim)',
                  padding: '2px 6px',
                }}
              >
                {[0, 1, 2, 3].map((m) => (
                  <option key={m} value={m}>
                    {MODE_LABELS[m]}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <div style={{ marginTop: '10px' }}>
            <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginBottom: '4px' }}>BRIGHTNESS (0-100)</div>
            <input
              type="range"
              min="0"
              max="100"
              value={localBrightness}
              onChange={(e) => setLocalBrightness(Number(e.target.value))}
              style={{ width: '100%', accentColor: 'var(--green-bright)' }}
            />
            <div style={{ fontSize: '11px', color: 'var(--green-bright)', marginTop: '2px' }}>
              {localBrightness}%
            </div>
          </div>
          <div style={{ marginTop: '12px' }}>
            <button
              type="button"
              onClick={applyConfig}
              disabled={loading}
              style={{
                background: 'var(--bg-void)',
                color: 'var(--green-bright)',
                border: '1px solid var(--green-dim)',
                fontFamily: 'var(--font-mono)',
                fontSize: '12px',
                padding: '6px 14px',
                cursor: loading ? 'not-allowed' : 'pointer',
                opacity: loading ? 0.6 : 1,
              }}
            >
              {loading ? 'APPLYING...' : 'APPLY CONFIG'}
            </button>
          </div>
        </div>

        {status && (
          <div style={{ marginTop: '12px', paddingTop: '8px', borderTop: '1px solid var(--green-dim)' }}>
            <div style={{ color: 'var(--green-dim)', fontSize: '11px', marginBottom: '8px' }}>
              ACTIVE STATE
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
              <span style={{ color: 'var(--green-dim)' }}>MODE</span>
              <span style={{ color: 'var(--green-bright)', fontWeight: 600 }}>
                {MODE_LABELS[active.mode] ?? `MODE ${active.mode}`}
              </span>
              <span style={{ color: 'var(--green-dim)' }}>RED</span>
              <span style={{ color: 'var(--green-bright)' }}>{active.red}</span>
              <span style={{ color: 'var(--green-dim)' }}>GREEN</span>
              <span style={{ color: 'var(--green-bright)' }}>{active.green}</span>
              <span style={{ color: 'var(--green-dim)' }}>BLUE</span>
              <span style={{ color: 'var(--green-bright)' }}>{active.blue}</span>
              <span style={{ color: 'var(--green-dim)' }}>BRIGHTNESS</span>
              <span style={{ color: 'var(--green-bright)' }}>{active.brightness}%</span>
            </div>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
