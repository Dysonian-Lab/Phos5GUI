import { OutdatedDeviceFirmwareModal, type FlashProgressPayload } from './OutdatedDeviceFirmwareModal';
import { OutdatedDesktopClientModal } from './OutdatedDesktopClientModal';

export type VersionMatchStatus =
  | 'Matched'
  | { DeviceOutdated: { device_ver: number; client_ver: number } }
  | { ClientOutdated: { device_ver: number; client_ver: number } }
  | { ProbeFailed: { reason: string } };

export interface VersionMismatchModalProps {
  status: VersionMatchStatus | null;
  isOpen: boolean;
  isProcessing?: boolean;
  progress?: FlashProgressPayload | null;
  errorMessage?: string | null;
  onClose: () => void;
  onUpdateFirmware: () => void;
  onCompatibilityMode: () => void;
  onUseLocalClient: () => void;
  onRebuildClient: () => void;
  onDownflash: () => void;
  onContinueAnyway: () => void;
}

export function VersionMismatchModal({
  status,
  isOpen,
  isProcessing = false,
  progress = null,
  errorMessage = null,
  onClose,
  onUpdateFirmware,
  onCompatibilityMode,
  onUseLocalClient,
  onRebuildClient,
  onDownflash,
  onContinueAnyway,
}: VersionMismatchModalProps) {
  if (!isOpen || !status || status === 'Matched') return null;
  if ('ProbeFailed' in status) return null;

  if ('DeviceOutdated' in status) {
    const { device_ver, client_ver } = status.DeviceOutdated;
    return (
      <OutdatedDeviceFirmwareModal
        deviceVer={device_ver}
        clientVer={client_ver}
        isOpen={isOpen}
        isProcessing={isProcessing}
        progress={progress}
        errorMessage={errorMessage}
        onClose={onClose}
        onUpdateFirmware={onUpdateFirmware}
        onCompatibilityMode={onCompatibilityMode}
      />
    );
  }

  if ('ClientOutdated' in status) {
    const { device_ver, client_ver } = status.ClientOutdated;
    return (
      <OutdatedDesktopClientModal
        deviceVer={device_ver}
        clientVer={client_ver}
        isOpen={isOpen}
        onClose={onClose}
        onUseLocalClient={onUseLocalClient}
        onRebuildClient={onRebuildClient}
        onDownflash={onDownflash}
        onContinueAnyway={onContinueAnyway}
      />
    );
  }

  return null;
}
