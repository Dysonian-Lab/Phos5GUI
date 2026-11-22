// XState v5 connection state machine for PM5 device connection.
// Orchestrates: idle -> probing -> evaluating_version -> version_mismatch_prompt -> flashing | connecting | connected | error
// Strict version logic: device_ver != client_ver -> version_mismatch_prompt -> flash bundled matching firmware.

import { setup, assign, fromPromise } from 'xstate';
import { invoke } from '@tauri-apps/api/core';
import type { VersionMatchStatus } from '../components/modals';

function stripSystemPaths(msg: string): string {
  return msg.replace(/[A-Z]:\\[\w\\.-]+/gi, '[path]').replace(/\/(?:home|Users|tmp|var)\/[\w/.-]+/g, '[path]');
}

function extractErrorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e && typeof e === 'object') {
    if ('message' in e) return String((e as { message: unknown }).message);
    return JSON.stringify(e);
  }
  return String(e);
}

export interface ConnectionContext {
  port: string | null;
  baudrate: number;
  capabilitiesVersion: number | null;
  clientVersion: number;
  versionMatchStatus: VersionMatchStatus | null;
  phase: 'idle' | 'probing' | 'evaluating_version' | 'version_mismatch_prompt' | 'flashing' | 'connecting' | 'connected' | 'error';
  errorMessage: string | null;
  flashProgress: { stage: string; percent: number; message: string } | null;
  isProcessing: boolean;
}

const initialContext: ConnectionContext = {
  port: null,
  baudrate: 115200,
  capabilitiesVersion: null,
  clientVersion: 13,
  versionMatchStatus: null,
  phase: 'idle',
  errorMessage: null,
  flashProgress: null,
  isProcessing: false,
};

export type ConnectionEvent =
  | { type: 'CONNECT'; port: string }
  | { type: 'PROBE_COMPLETE'; capabilitiesVersion: number }
  | { type: 'VERSION_EVALUATED'; status: VersionMatchStatus }
  | { type: 'FLASH_START' }
  | { type: 'FLASH_PROGRESS'; stage: string; percent: number; message: string }
  | { type: 'FLASH_COMPLETE' }
  | { type: 'FLASH_FAILED'; message: string }
  | { type: 'CONNECTED' }
  | { type: 'CANCEL_FLASH' }
  | { type: 'RETRY' }
  | { type: 'DISCONNECT' }
  | { type: 'RESET' };

export const connectionMachine = setup({
  types: {} as {
    context: ConnectionContext;
    events: ConnectionEvent;
  },
  actors: {
    probeCapabilities: fromPromise<{ capabilitiesVersion: number }, ConnectionContext>(async ({ input }) => {
      if (!input.port) throw new Error('No device port available');
      // Invoke Rust backend direct serial probe (Layer 1: CMD_PING + CMD_CAPABILITIES)
      const result = await invoke<{ capabilities_version: number }>('probe_device_capabilities', {
        port: input.port,
        baudrate: input.baudrate,
      });
      return { capabilitiesVersion: result.capabilities_version };
    }),
    evaluateVersion: fromPromise<VersionMatchStatus, ConnectionContext>(async ({ input }) => {
      if (!input.port) throw new Error('No device port available');
      // Invoke Rust backend strict version comparison
      return invoke<VersionMatchStatus>('evaluate_version_match', { port: input.port });
    }),
    flashFirmware: fromPromise<void, ConnectionContext & { variant?: string }>(async ({ input }) => {
      if (!input.port) throw new Error('No device port available');
      const variant = input.variant || 'standard';
      await invoke<void>('flash_pm5_firmware', { port: input.port, variant });
    }),
  },
}).createMachine({
  id: 'connection',
  initial: 'idle',
  context: initialContext,

  states: {
    idle: {
      on: {
        CONNECT: {
          target: 'probing',
          actions: assign({
            port: ({ event }) => event.port,
            errorMessage: () => null,
            phase: () => 'probing' as const,
            isProcessing: () => true,
          }),
        },
        RESET: { actions: assign(() => initialContext) },
      },
    },

    probing: {
      invoke: {
        src: 'probeCapabilities',
        input: ({ context }: { context: ConnectionContext }) => context,
        onDone: [
          {
            target: 'evaluating_version',
            actions: assign({
              capabilitiesVersion: ({ event }) => event.output.capabilitiesVersion,
              phase: () => 'evaluating_version' as const,
            }),
          },
        ],
        onError: {
          target: 'error',
          actions: assign({
            errorMessage: ({ event }) => stripSystemPaths(extractErrorMessage(event.error)),
            phase: () => 'error' as const,
            isProcessing: () => false,
          }),
        },
      },
      on: {
        RESET: { target: 'idle', actions: assign(() => initialContext) },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    evaluating_version: {
      invoke: {
        src: 'evaluateVersion',
        input: ({ context }: { context: ConnectionContext }) => context,
        onDone: [
          {
            target: 'version_mismatch_prompt',
            actions: assign({
              versionMatchStatus: ({ event }) => event.output,
              phase: () => 'version_mismatch_prompt' as const,
            }),
          },
        ],
        onError: {
          target: 'error',
          actions: assign({
            errorMessage: ({ event }) => stripSystemPaths(extractErrorMessage(event.error)),
            phase: () => 'error' as const,
            isProcessing: () => false,
          }),
        },
      },
      on: {
        RESET: { target: 'idle', actions: assign(() => initialContext) },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    version_mismatch_prompt: {
      // Strict version policy: device_ver != client_ver requires flashing
      // bundled matching firmware before connecting.
      on: {
        FLASH_START: {
          target: 'flashing',
          actions: assign({
            phase: () => 'flashing' as const,
            flashProgress: () => ({ stage: 'start', percent: 0, message: 'Flashing bundled PM5 firmware...' }),
            isProcessing: () => true,
          }),
        },
        CONNECTED: {
          target: 'connected',
          actions: assign({
            phase: () => 'connected' as const,
            isProcessing: () => false,
          }),
        },
        RETRY: {
          target: 'probing',
          actions: assign({
            phase: () => 'probing' as const,
            errorMessage: () => null,
          }),
        },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
        RESET: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    flashing: {
      invoke: {
        src: 'flashFirmware',
        input: ({ context }: { context: ConnectionContext }) => context,
        onDone: [
          {
            target: 'connecting',
            actions: assign({
              flashProgress: () => ({ stage: 'done', percent: 100, message: 'Firmware flash complete.' }),
              phase: () => 'connecting' as const,
            }),
          },
        ],
        onError: {
          target: 'error',
          actions: assign({
            errorMessage: ({ event }) => stripSystemPaths(extractErrorMessage(event.error)),
            flashProgress: ({ event }) => ({ stage: 'error', percent: 0, message: event.error as string }),
            phase: () => 'error' as const,
            isProcessing: () => false,
          }),
        },
      },
      on: {
        FLASH_PROGRESS: {
          actions: assign({
            flashProgress: ({ event }) => ({
              stage: event.stage,
              percent: event.percent,
              message: event.message,
            }),
          }),
        },
        FLASH_COMPLETE: {
          target: 'connecting',
          actions: assign({
            flashProgress: () => ({ stage: 'done', percent: 100, message: 'Firmware flash complete.' }),
            phase: () => 'connecting' as const,
          }),
        },
        FLASH_FAILED: {
          target: 'error',
          actions: assign({
            errorMessage: ({ event }) => event.message,
            phase: () => 'error' as const,
            isProcessing: () => false,
          }),
        },
        CANCEL_FLASH: {
          target: 'version_mismatch_prompt',
          actions: assign({
            phase: () => 'version_mismatch_prompt' as const,
            flashProgress: () => null,
            isProcessing: () => false,
          }),
        },
        RESET: { target: 'idle', actions: assign(() => initialContext) },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    connecting: {
      on: {
        CONNECTED: {
          target: 'connected',
          actions: assign({
            phase: () => 'connected' as const,
            isProcessing: () => false,
          }),
        },
        RESET: { target: 'idle', actions: assign(() => initialContext) },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    connected: {
      on: {
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
        RESET: { target: 'idle', actions: assign(() => initialContext) },
      },
    },

    error: {
      on: {
        RETRY: {
          target: 'probing',
          actions: assign({
            phase: () => 'probing' as const,
            errorMessage: () => null,
            isProcessing: () => true,
          }),
        },
        DISCONNECT: { target: 'idle', actions: assign(() => initialContext) },
        RESET: { target: 'idle', actions: assign(() => initialContext) },
      },
    },
  },
});
