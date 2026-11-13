import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { VersionMatchStatus } from '../components/modals';

export type Pm5FirmwareVariant = 'standard' | 'bwm';

export interface FlashProgressPayload {
  stage: string;
  percent: number;
  message: string;
}

export interface UseVersionMediationReturn {
  status: VersionMatchStatus | null;
  isOpen: boolean;
  isProcessing: boolean;
  errorMessage: string | null;
  flashProgress: FlashProgressPayload | null;
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
  const [flashProgress, setFlashProgress] = useState<FlashProgressPayload | null>(null);

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
    async (port: string, variant: Pm5FirmwareVariant = 'bwm'): Promise<void> => {
      setIsProcessing(true);
      setErrorMessage(null);
      setFlashProgress({ stage: 'start', percent: 0, message: 'Starting PM5 provisioning...' });
      let unlisten: UnlistenFn | null = null;
      try {
        unlisten = await listen<FlashProgressPayload>('flash-progress', (event) => {
          setFlashProgress(event.payload);
        });
        await invoke<string>('flash_pm5_complete', { port, variant });
        setFlashProgress((prev) =>
          prev ?? { stage: 'done', percent: 100, message: 'Provisioning complete.' },
        );
        setIsOpen(false);
      } catch (error) {
        setErrorMessage(getErrorMessage(error));
      } finally {
        if (unlisten) {
          unlisten();
        }
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
    flashProgress,
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
