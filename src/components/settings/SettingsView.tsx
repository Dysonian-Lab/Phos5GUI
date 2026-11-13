import { useEffect, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useSettings } from '../../hooks/useSettings';
import { useSfx } from '../../hooks/useSfx';
import { useWizard } from '../../hooks/useWizard';
import { TerminalPanel } from '../shared/TerminalPanel';

type FlashProgressPayload = {
  stage: string;
  percent: number;
  message: string;
};

export function SettingsView() {
  const { settings, updateSettings } = useSettings();
  const sfx = useSfx();
  const wizard = useWizard();

  const [isProvisioning, setIsProvisioning] = useState(false);
  const [progress, setProgress] = useState<FlashProgressPayload | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    (async () => {
      unlisten = await listen<FlashProgressPayload>('flash-progress', (event) => {
        setProgress(event.payload);
      });
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const toggleExpert = () => {
    sfx.click();
    updateSettings({ expertMode: !settings.expertMode });
  };

  const startProvision = async () => {
    if (!wizard.context.port) {
      setError('No device port detected. Connect a PM5 device first.');
      return;
    }
    sfx.action();
    setIsProvisioning(true);
    setError(null);
    setResult(null);
    setProgress({ stage: 'start', percent: 0, message: 'Starting PM5 provisioning...' });
    try {
      const res = await invoke<string>('flash_pm5_complete', {
        port: wizard.context.port,
        variant: 'bwm',
      });
      setResult(res);
      setProgress((p) => p ?? { stage: 'verify', percent: 100, message: 'Provisioning complete.' });
    } catch (e) {
      const msg = typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
      setError(msg);
    } finally {
      setIsProvisioning(false);
    }
  };

  const statusText = settings.expertMode ? '[ON]' : '[OFF]';
  const statusColor = settings.expertMode ? 'var(--green-bright)' : 'var(--green-dim)';
  const pct = Math.max(0, Math.min(100, progress?.percent ?? 0));

  return (
    <TerminalPanel title="SETTINGS">
      <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
        <div>
          <div style={{ color: 'var(--green-mid)', fontSize: '13px', fontWeight: 600 }}>
            EXPERT MODE
          </div>
          <div style={{ color: 'var(--green-dim)', fontSize: '12px', marginTop: '4px' }}>
            Allow raw PM3 command input in terminal
          </div>
          <div style={{ marginTop: '8px', fontSize: '13px' }}>
            <span style={{ color: 'var(--green-mid)' }}>STATUS: </span>
            <span
              onClick={toggleExpert}
              onMouseEnter={(e) => {
                sfx.hover();
                e.currentTarget.style.textShadow = '0 0 6px var(--green-bright)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.textShadow = 'none';
              }}
              style={{
                color: statusColor,
                cursor: 'pointer',
                userSelect: 'none',
                fontWeight: 600,
                transition: 'color 0.15s, text-shadow 0.15s',
              }}
            >
              {statusText}
            </span>
          </div>
        </div>

        <div style={{ borderTop: '1px solid var(--green-dim)', paddingTop: '12px' }}>
          <div style={{ color: 'var(--green-mid)', fontSize: '13px', fontWeight: 600 }}>
            PM5 FLASH / PROVISION
          </div>
          <div style={{ color: 'var(--green-dim)', fontSize: '12px', marginTop: '4px', lineHeight: 1.6 }}>
            Multi-stage provisioning (BWM): [1/4] bootloader, [2/4] bootrom+fullimage, [3/4] FPGA bitstream, [4/4] BWM/FPGA verification.
          </div>
          <div style={{ marginTop: '10px', display: 'flex', gap: '10px', alignItems: 'center', flexWrap: 'wrap' }}>
            <button
              type="button"
              onClick={() => void startProvision()}
              disabled={isProvisioning}
              style={{
                background: 'var(--bg-void)',
                color: 'var(--green-bright)',
                border: '2px solid var(--green-bright)',
                fontFamily: 'var(--font-mono)',
                fontSize: '12px',
                fontWeight: 600,
                padding: '6px 14px',
                cursor: isProvisioning ? 'not-allowed' : 'pointer',
                opacity: isProvisioning ? 0.6 : 1,
                textTransform: 'uppercase',
              }}
            >
              {isProvisioning ? 'Provisioning...' : 'Flash / Provision Device'}
            </button>
            <span style={{ color: 'var(--green-dim)', fontSize: '11px' }}>
              PORT: {wizard.context.port ?? '---'}
            </span>
          </div>

          {(isProvisioning || progress) && (
            <div style={{ marginTop: '10px' }}>
              <div style={{ fontSize: '12px', color: 'var(--green-mid)', marginBottom: '6px' }}>
                {progress?.message ?? 'Waiting...'}
              </div>
              <div style={{ height: '10px', border: '1px solid var(--green-dim)', background: 'var(--bg-void)' }}>
                <div style={{ height: '100%', width: `${pct}%`, background: 'var(--green-bright)', transition: 'width 150ms linear' }} />
              </div>
              <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginTop: '4px' }}>
                {pct}% {progress?.stage ? `· ${progress.stage}` : ''}
              </div>
            </div>
          )}

          {result && (
            <div style={{ marginTop: '10px', color: 'var(--green-bright)', fontSize: '12px', whiteSpace: 'pre-wrap' }}>
              [+] {result}
            </div>
          )}
          {error && (
            <div style={{ marginTop: '10px', color: 'var(--red-bright, #FF0033)', fontSize: '12px', whiteSpace: 'pre-wrap' }}>
              [!!] {error}
            </div>
          )}
        </div>
      </div>
    </TerminalPanel>
  );
}
