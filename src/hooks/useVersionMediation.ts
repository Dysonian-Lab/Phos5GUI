import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { VersionMatchStatus } from '../components/modals';

export type Pm5FirmwareVariant = 'standard' | 'bwm';

export interface UseVersionMediationReturn {
  status: VersionMatchStatus | null;
  isOpen: boolean;
  isProcessing: boolean;
  errorMessage: string | null;
  checkVersion: (port: string) => Promise<VersionMatchStatus | null>;
  handleUpdateFirmware: (port: string, variant?: Pm5FirmwareVariant) => Promise<void>;
  handleCompatibilityMode: () => void;
  handleUseLocalClient: () => void;
  handleRebuildClient: () => void;
  handleDownflash: () => void;
  handleContinueAnyway: () => void;
  closeModal: () => void;
}

function getErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null) {
    const values = Object.values(error as Record<string, unknown>);
    const first = values[0];
    if (typeof first === 'string') return first;
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  return String(error);
}

export function useVersionMediation(): UseVersionMediationReturn {
  const [status, setStatus] = useState<VersionMatchStatus | null>(null);
  const [isOpen, setIsOpen] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const closeModal = useCallback(() => {
    setIsOpen(false);
  }, []);

  const checkVersion = useCallback(async (port: string): Promise<VersionMatchStatus | null> => {
    setIsProcessing(true);
    setErrorMessage(null);
    try {
      const result = await invoke<VersionMatchStatus>('evaluate_version_match', { port });
      setStatus(result);

      if (typeof result === 'object' && result !== null) {
        if ('DeviceOutdated' in result || 'ClientOutdated' in result) {
          setIsOpen(true);
        } else {
          setIsOpen(false);
        }
      } else if (result === 'Matched') {
        setIsOpen(false);
      } else {
        setIsOpen(false);
      }

      return result;
    } catch (error) {
      const message = getErrorMessage(error);
      setErrorMessage(message);
      setStatus({ ProbeFailed: { reason: message } });
      setIsOpen(false);
      return null;
    } finally {
      setIsProcessing(false);
    }
  }, []);

  const handleUpdateFirmware = useCallback(
    async (port: string, variant: Pm5FirmwareVariant = 'standard'): Promise<void> => {
      setIsProcessing(true);
      setErrorMessage(null);
      try {
        await invoke<void>('flash_pm5_firmware', { port, variant });
        setIsOpen(false);
      } catch (error) {
        setErrorMessage(getErrorMessage(error));
      } finally {
        setIsProcessing(false);
      }
    },
    [],
  );

  const handleCompatibilityMode = useCallback(() => {
    setErrorMessage(null);
    setIsOpen(false);
  }, []);

  const handleUseLocalClient = useCallback(() => {
    // TODO: wire to backend client-override command when available.
    setErrorMessage(null);
  }, []);

  const handleRebuildClient = useCallback(() => {
    // TODO: wire to backend rebuild command when available.
    setErrorMessage(null);
  }, []);

  const handleDownflash = useCallback(() => {
    // TODO: wire to bundled downflash action when available.
    setErrorMessage(null);
  }, []);

  const handleContinueAnyway = useCallback(() => {
    setErrorMessage(null);
    setIsOpen(false);
  }, []);

  return {
    status,
    isOpen,
    isProcessing,
    errorMessage,
    checkVersion,
    handleUpdateFirmware,
    handleCompatibilityMode,
    handleUseLocalClient,
    handleRebuildClient,
    handleDownflash,
    handleContinueAnyway,
    closeModal,
  };
}
