import type { CSSProperties } from 'react';
import type { VersionMatchStatus } from './modals';
import type { UiFeatureSet } from '../utils/featureGating';

export interface DegradedModeBannerProps {
  status: VersionMatchStatus | null;
  featureSet: UiFeatureSet;
  onOpenDetails?: () => void;
}

const bannerStyle: CSSProperties = {
  width: '100%',
  background: 'var(--bg-panel, #0A0A0A)',
  border: '1px solid var(--amber, #FFB800)',
  color: 'var(--amber, #FFB800)',
  fontFamily: 'var(--font-mono)',
  fontSize: '12px',
  lineHeight: 1.6,
  padding: '8px 12px',
  textTransform: 'none',
  boxSizing: 'border-box',
};

export function DegradedModeBanner({
  status,
  featureSet,
  onOpenDetails,
}: DegradedModeBannerProps) {
  if (!featureSet.isDegradedMode || status === 'Matched' || status === null) {
    return null;
  }

  let message = 'Warning: Could not verify device capabilities version.';

  if ('DeviceOutdated' in status) {
    const { device_ver } = status.DeviceOutdated;
    message = `Running in degraded mode (Device v${device_ver}) — Some features may be unavailable.`;
  } else if ('ClientOutdated' in status) {
    const { device_ver } = status.ClientOutdated;
    message = `Running with newer hardware firmware (Device v${device_ver}) — Proceeding in compatibility mode.`;
  } else if ('ProbeFailed' in status) {
    message = 'Warning: Could not verify device capabilities version.';
  }

  const interactive: CSSProperties = onOpenDetails
    ? { cursor: 'pointer' }
    : {};

  return (
    <div
      style={{ ...bannerStyle, ...interactive }}
      onClick={onOpenDetails}
      role={onOpenDetails ? 'button' : undefined}
      tabIndex={onOpenDetails ? 0 : undefined}
      onKeyDown={
        onOpenDetails
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onOpenDetails();
              }
            }
          : undefined
      }
    >
      [!] {message}
    </div>
  );
}
