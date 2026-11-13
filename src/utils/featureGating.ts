import type { VersionMatchStatus } from '../components/modals';

export interface UiFeatureSet {
  /** BWM panel access (requires capabilities version >= 12). */
  canAccessBwm: boolean;
  /** CEP panel access (requires capabilities version >= 13). */
  canAccessCep: boolean;
  /** FPGA management access (default true for PM5). */
  canAccessFpga: boolean;
  /** Indicates compatibility/degraded mediation mode is active. */
  isDegradedMode: boolean;
}

export function deriveUiFeatures(status: VersionMatchStatus | null): UiFeatureSet {
  if (status === null || status === 'Matched') {
    return {
      canAccessBwm: true,
      canAccessCep: true,
      canAccessFpga: true,
      isDegradedMode: false,
    };
  }

  if ('DeviceOutdated' in status) {
    const { device_ver } = status.DeviceOutdated;
    return {
      canAccessBwm: device_ver >= 12,
      canAccessCep: device_ver >= 13,
      canAccessFpga: true,
      isDegradedMode: true,
    };
  }

  if ('ClientOutdated' in status) {
    // Compatibility passthrough: keep features enabled but mark degraded.
    return {
      canAccessBwm: true,
      canAccessCep: true,
      canAccessFpga: true,
      isDegradedMode: true,
    };
  }

  if ('ProbeFailed' in status) {
    return {
      canAccessBwm: false,
      canAccessCep: false,
      canAccessFpga: true,
      isDegradedMode: true,
    };
  }

  // Exhaustive fallback (defensive).
  return {
    canAccessBwm: false,
    canAccessCep: false,
    canAccessFpga: true,
    isDegradedMode: true,
  };
}
