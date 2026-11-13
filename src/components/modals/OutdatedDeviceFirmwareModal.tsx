import type { CSSProperties } from 'react';

export interface FlashProgressPayload {
  stage: string;
  percent: number;
  message: string;
}

export interface OutdatedDeviceFirmwareModalProps {
  deviceVer: number;
  clientVer: number;
  isOpen: boolean;
  isProcessing?: boolean;
  progress?: FlashProgressPayload | null;
  errorMessage?: string | null;
  onClose: () => void;
  onUpdateFirmware: () => void;
  onCompatibilityMode: () => void;
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
  border: '2px solid var(--amber, #FFB800)',
  padding: '20px 24px',
  width: 'min(620px, calc(100vw - 48px))',
  fontFamily: 'var(--font-mono)',
  color: 'var(--green-bright, #00FF41)',
};

const titleStyle: CSSProperties = {
  color: 'var(--amber, #FFB800)',
  fontSize: '15px',
  fontWeight: 700,
  marginBottom: '12px',
  textTransform: 'uppercase',
};

const bodyStyle: CSSProperties = {
  fontSize: '13px',
  lineHeight: 1.7,
  color: 'var(--green-mid, #00CC33)',
  marginBottom: '16px',
};

const actionsStyle: CSSProperties = {
  display: 'flex',
  gap: '12px',
  flexWrap: 'wrap',
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

const disabledBtn: CSSProperties = {
  opacity: 0.6,
  cursor: 'not-allowed',
};

export function OutdatedDeviceFirmwareModal({
  deviceVer,
  clientVer,
  isOpen,
  isProcessing = false,
  progress = null,
  errorMessage = null,
  onClose,
  onUpdateFirmware,
  onCompatibilityMode,
}: OutdatedDeviceFirmwareModalProps) {
  if (!isOpen) return null;

  const percent = Math.max(0, Math.min(100, progress?.percent ?? 0));

  return (
    <div style={overlayStyle} onClick={isProcessing ? undefined : onClose} role="dialog" aria-modal="true">
      <div style={panelStyle} onClick={(e) => e.stopPropagation()}>
        <div style={titleStyle}>Outdated Device Firmware Detected</div>
        <div style={bodyStyle}>
          Your Proxmark5 is running firmware capabilities v{deviceVer}, but this client expects v{clientVer}.
          Some commands and features may be unavailable or unstable.
        </div>

        {(isProcessing || progress) && (
          <div style={{ marginBottom: '14px' }}>
            <div style={{ fontSize: '12px', color: 'var(--green-mid)', marginBottom: '6px' }}>
              {progress?.message ?? '[1/4] Preparing PM5 provisioning...'}
            </div>
            <div style={{ height: '10px', border: '1px solid var(--green-dim)', background: 'var(--bg-void)' }}>
              <div
                style={{
                  height: '100%',
                  width: `${percent}%`,
                  background: 'var(--green-bright)',
                  transition: 'width 150ms linear',
                }}
              />
            </div>
            <div style={{ fontSize: '11px', color: 'var(--green-dim)', marginTop: '4px' }}>
              {percent}% {progress?.stage ? `· ${progress.stage}` : ''}
            </div>
          </div>
        )}

        {errorMessage && (
          <div style={{ marginBottom: '12px', color: 'var(--red-bright, #FF0033)', fontSize: '12px' }}>
            [!!] {errorMessage}
          </div>
        )}

        <div style={actionsStyle}>
          <button
            type="button"
            style={{ ...primaryBtn, ...(isProcessing ? disabledBtn : {}) }}
            onClick={onUpdateFirmware}
            disabled={isProcessing}
          >
            {isProcessing ? 'Updating PM5...' : 'Update PM5 Firmware'}
          </button>
          <button
            type="button"
            style={{ ...secondaryBtn, ...(isProcessing ? disabledBtn : {}) }}
            onClick={onCompatibilityMode}
            disabled={isProcessing}
          >
            Continue in Compatibility Mode
          </button>
        </div>
      </div>
    </div>
  );
}
