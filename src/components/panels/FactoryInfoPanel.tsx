import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { TerminalPanel } from '../shared/TerminalPanel';

export interface FactoryInfo {
  serial_number: string;
  batch_id: string;
  production_date: string;
  qc_passed: boolean;
}

export interface FactoryInfoPanelProps {
  port: string;
}

export function FactoryInfoPanel({ port }: FactoryInfoPanelProps) {
  const [info, setInfo] = useState<FactoryInfo | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const requestSeq = useRef(0);

  const fetchInfo = useCallback(
    async (manual = false) => {
      if (!port) return;
      const seq = ++requestSeq.current;
      if (manual) setLoading(true);
      setError(null);
      try {
        const result = await invoke<FactoryInfo>('get_factory_info', { port });
        if (seq !== requestSeq.current) return;
        setInfo(result);
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
    setInfo(null);
    setError(null);
    setLoading(false);
    if (port) {
      void fetchInfo(false);
    }
  }, [port, fetchInfo]);

  const qcPassed = info?.qc_passed ?? false;

  return (
    <TerminalPanel title="FACTORY INFO">
      <div style={{ fontFamily: 'var(--font-mono)', fontSize: '13px', color: 'var(--green-bright)' }}>
        <div style={{ display: 'flex', gap: '8px', marginBottom: '10px', flexWrap: 'wrap', alignItems: 'center' }}>
          <button
            type="button"
            onClick={() => void fetchInfo(true)}
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
          <span style={{ color: 'var(--green-dim)', fontSize: '11px' }}>
            {port ? `PORT ${port}` : 'NO PORT'}
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

        {!info && !error && (
          <div style={{ color: 'var(--green-dim)', fontSize: '12px' }}>
            {loading ? 'Querying factory info...' : 'No info yet.'}
          </div>
        )}

        {info && (
          <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '4px 12px', fontSize: '12px' }}>
            <span style={{ color: 'var(--green-dim)' }}>SERIAL NUMBER</span>
            <span style={{ color: 'var(--green-bright)' }}>{info.serial_number || 'N/A'}</span>
            <span style={{ color: 'var(--green-dim)' }}>BATCH ID</span>
            <span style={{ color: 'var(--green-bright)' }}>{info.batch_id || 'N/A'}</span>
            <span style={{ color: 'var(--green-dim)' }}>PRODUCTION DATE</span>
            <span style={{ color: 'var(--green-bright)' }}>{info.production_date || 'N/A'}</span>
            <span style={{ color: 'var(--green-dim)' }}>QC STATUS</span>
            <span
              style={{
                padding: '2px 8px',
                border: `1px solid ${qcPassed ? 'var(--green-bright)' : 'var(--amber)'}`,
                color: qcPassed ? 'var(--green-bright)' : 'var(--amber)',
                fontSize: '11px',
                fontWeight: 600,
                background: 'var(--bg-void)',
              }}
            >
              {qcPassed ? 'PASSED' : 'FAILED'}
            </span>
          </div>
        )}
      </div>
    </TerminalPanel>
  );
}
