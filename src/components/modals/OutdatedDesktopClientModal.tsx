import type { CSSProperties } from 'react';

export interface OutdatedDesktopClientModalProps {
  deviceVer: number;
  clientVer: number;
  isOpen: boolean;
  onClose: () => void;
  onUseLocalClient: () => void;
  onRebuildClient: () => void;
  onDownflash: () => void;
  onContinueAnyway: () => void;
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

const tertiaryBtn: CSSProperties = {
  ...primaryBtn,
  color: 'var(--green-dim, #00801A)',
  border: '2px solid var(--green-dim, #00801A)',
};

export function OutdatedDesktopClientModal({
  deviceVer,
  clientVer,
  isOpen,
  onClose,
  onUseLocalClient,
  onRebuildClient,
  onDownflash,
  onContinueAnyway,
}: OutdatedDesktopClientModalProps) {
  if (!isOpen) return null;

  return (
    <div style={overlayStyle} onClick={onClose} role="dialog" aria-modal="true">
      <div style={panelStyle} onClick={(e) => e.stopPropagation()}>
        <div style={titleStyle}>Outdated Desktop Client Detected</div>
        <div style={bodyStyle}>
          Your Proxmark5 is running newer firmware capabilities (v{deviceVer}) than this Phos5GUI release (v{clientVer}).
        </div>
        <div style={actionsStyle}>
          <button type="button" style={primaryBtn} onClick={onUseLocalClient}>
            Use Local Client
          </button>
          <button type="button" style={secondaryBtn} onClick={onRebuildClient}>
            Rebuild Matching Client
          </button>
          <button type="button" style={secondaryBtn} onClick={onDownflash}>
            Downflash Firmware
          </button>
          <button type="button" style={tertiaryBtn} onClick={onContinueAnyway}>
            Continue Anyway
          </button>
        </div>
      </div>
    </div>
  );
}
