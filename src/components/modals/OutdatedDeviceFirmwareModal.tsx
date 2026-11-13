import type { CSSProperties } from 'react';

export interface OutdatedDeviceFirmwareModalProps {
  deviceVer: number;
  clientVer: number;
  isOpen: boolean;
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
  width: 'min(560px, calc(100vw - 48px))',
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

export function OutdatedDeviceFirmwareModal({
  deviceVer,
  clientVer,
  isOpen,
  onClose,
  onUpdateFirmware,
  onCompatibilityMode,
}: OutdatedDeviceFirmwareModalProps) {
  if (!isOpen) return null;

  return (
    <div style={overlayStyle} onClick={onClose} role="dialog" aria-modal="true">
      <div style={panelStyle} onClick={(e) => e.stopPropagation()}>
        <div style={titleStyle}>Outdated Device Firmware Detected</div>
        <div style={bodyStyle}>
          Your Proxmark5 is running firmware capabilities v{deviceVer}, but this client expects v{clientVer}.
          Some commands and features may be unavailable or unstable.
        </div>
        <div style={actionsStyle}>
          <button type="button" style={primaryBtn} onClick={onUpdateFirmware}>
            Update PM5 Firmware
          </button>
          <button type="button" style={secondaryBtn} onClick={onCompatibilityMode}>
            Continue in Compatibility Mode
          </button>
        </div>
      </div>
    </div>
  );
}
