import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { TopBar } from './TopBar';
import { Sidebar, type TabId } from './Sidebar';
import { StatusBar, type SystemStatus } from './StatusBar';
import { WizardContainer } from '../wizard/WizardContainer';
import { EraseView } from '../erase/EraseView';
import { HistoryView } from '../history/HistoryView';
import { SavedView } from '../saved/SavedView';
import { SettingsView } from '../settings/SettingsView';
import { AdvancedContainer } from '../advanced/AdvancedContainer';
import { useMusic } from '../../hooks/useMusic';
import { useWizard } from '../../hooks/useWizard';
import { LiveTerminal } from '../shared/LiveTerminal';
import { useVersionMediation } from '../../hooks/useVersionMediation';
import { deriveUiFeatures, type UiFeatureSet } from '../../utils/featureGating';
import { DegradedModeBanner } from '../DegradedModeBanner';
import {
  OutdatedDeviceFirmwareModal,
  OutdatedDesktopClientModal,
  VersionMismatchModal,
} from '../modals';
import {
  BwmPanel,
  CepPanel,
  FpgaPanel,
  RgbPanel,
  PowerPanel,
  FactoryInfoPanel,
  AntennaPanel,
  IcsDecoderPanel,
} from '../panels';

export function MainLayoutShell() {
  const [activeTab, setActiveTab] = useState<TabId>('scan');
  const { enabled: musicEnabled, toggle: toggleMusic } = useMusic();
  const wizard = useWizard();
  const {
    status: versionStatus,
    checkVersion,
    isProcessing: isProvisioning,
    flashProgress,
    errorMessage: provisionError,
    handleUpdateFirmware,
  } = useVersionMediation();

  // B1: Derive PM3 connection status from wizard step
  const connected = wizard.currentStep !== 'Idle' && wizard.currentStep !== 'DetectingDevice';

  // Auto-connect on startup: probe for a PM3 device once when the shell mounts.
  // Reuses the existing `detect_device` Tauri command and wizard `DETECT` transition
  // (same path as the manual CONNECT button in ConnectStep). No reinvention.
  const hasAttemptedAutoDetect = useRef(false);
  useEffect(() => {
    if (hasAttemptedAutoDetect.current) return;
    if (wizard.isLoading) return;
    if (wizard.context.port !== null) return;
    if (wizard.context.errorMessage) return;
    hasAttemptedAutoDetect.current = true;
    wizard.detect();
  }, [wizard.detect, wizard.isLoading, wizard.context.port, wizard.context.errorMessage]);

  // Determine whether the connected device is a PM5.
  const isPm5 = wizard.context.model ? String(wizard.context.model).toUpperCase().includes('PM5') : false;

  // Derive PM5 feature gating from the version-match status.
  const featureSet: UiFeatureSet = useMemo(
    () => deriveUiFeatures(versionStatus),
    [versionStatus],
  );

  // Kick off a version match check once a port is known so feature flags reflect reality.
  const port = wizard.context.port ?? null;
  useEffect(() => {
    if (port && versionStatus === null) {
      void checkVersion(port);
    }
  }, [port, versionStatus, checkVersion]);

  // Local dismissal state so a version-mismatch modal is shown once per
  // (port, version) pair and can be dismissed without unmounting the shell.
  const [isModalDismissed, setIsModalDismissed] = useState<boolean>(false);
  useEffect(() => {
    setIsModalDismissed(false);
  }, [port, versionStatus]);

  // B8: Derive system status from wizard state
  const status: SystemStatus = wizard.isLoading ? 'busy'
    : wizard.currentStep === 'Error' ? 'error'
    : 'ready';

  const statusMessage = (() => {
    switch (wizard.currentStep) {
      case 'DetectingDevice': return 'Detecting PM3...';
      case 'ScanningCard': return 'Scanning card...';
      case 'WaitingForBlank': return 'Waiting for blank...';
      case 'Writing': return 'Writing clone...';
      case 'Verifying': return 'Verifying...';
      case 'Error': return wizard.context.errorUserMessage || 'ERROR';
      case 'Complete': return 'Clone complete!';
      default: return undefined;
    }
  })();

  const isDegraded = featureSet.isDegradedMode && versionStatus !== null && versionStatus !== 'Matched';

  const renderContent = useCallback(() => {
    switch (activeTab) {
      case 'scan':
      case 'write':
        return <WizardContainer />;
      case 'erase':
        return (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5, display: 'flex', justifyContent: 'center' }}>
            <EraseView port={wizard.context.port ?? undefined} />
          </div>
        );
      case 'history':
        return (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <HistoryView />
          </div>
        );
      case 'saved':
        return (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <SavedView />
          </div>
        );
      case 'settings':
        return (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <SettingsView />
          </div>
        );
      case 'advanced':
        return <AdvancedContainer />;
      case 'bwm':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <BwmPanel port={wizard.context.port ?? ''} canAccessBwm={featureSet.canAccessBwm} />
          </div>
        ) : null;
      case 'cep':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <CepPanel port={wizard.context.port ?? ''} canAccessCep={featureSet.canAccessCep} />
          </div>
        ) : null;
      case 'fpga':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <FpgaPanel port={wizard.context.port ?? ''} canAccessFpga={featureSet.canAccessFpga} />
          </div>
        ) : null;
      case 'rgb':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <RgbPanel port={wizard.context.port ?? ''} />
          </div>
        ) : null;
      case 'power':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <PowerPanel port={wizard.context.port ?? ''} />
          </div>
        ) : null;
      case 'factory':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <FactoryInfoPanel port={wizard.context.port ?? ''} />
          </div>
        ) : null;
      case 'antenna':
        return isPm5 ? (
          <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
            <AntennaPanel port={wizard.context.port ?? ''} />
          </div>
        ) : null;
      case 'ics':
        return isPm5 ? (
          <IcsDecoderPanel />
        ) : null;
      default:
        return null;
    }
  }, [activeTab, isPm5, featureSet.canAccessCep, featureSet.canAccessFpga, wizard.context.port]);

  const degradedBanner = isDegraded ? (
    <DegradedModeBanner
      status={versionStatus}
      featureSet={featureSet}
    />
  ) : null;

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateRows: '32px 1fr auto auto 24px',
        gridTemplateColumns: '180px 1fr',
        width: '100%',
        height: '100%',
        position: 'relative',
        zIndex: 10,
        background: 'var(--bg-void)',
      }}
    >
      {/* TopBar spans full width */}
      <div style={{ gridColumn: '1 / -1', gridRow: '1' }}>
        <TopBar connected={connected} onDisconnect={wizard.disconnect} />
      </div>

      {/* Sidebar */}
      <div style={{ gridRow: '2', gridColumn: '1' }}>
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          deviceName={wizard.context.model || '---'}
          devicePort={wizard.context.port || '---'}
          firmware={wizard.context.firmware || '---'}
          isPm5={isPm5}
          featureSet={featureSet}
        />
      </div>

      {/* Degraded-mode banner (content width) */}
      <div style={{ gridColumn: '1 / -1', gridRow: '3' }}>{degradedBanner}</div>

      {/* Main content area */}
      <div
        style={{
          gridRow: '2',
          gridColumn: '2',
          overflow: 'auto',
          position: 'relative',
          zIndex: 10,
          background: 'var(--bg-void)',
          width: '100%',
          height: '100%',
        }}
      >
        {renderContent()}
      </div>

      {/* LiveTerminal spans full width */}
      <div style={{ gridColumn: '1 / -1', gridRow: '4' }}>
        <LiveTerminal />
      </div>

      {/* StatusBar spans full width */}
      <div style={{ gridColumn: '1 / -1', gridRow: '5' }}>
        <StatusBar status={status} message={statusMessage} musicEnabled={musicEnabled} onMusicToggle={toggleMusic} />
      </div>

      {/* Version-mismatch mediation modals (render once per port/version pair) */}
      {(() => {
        if (isModalDismissed || versionStatus === null || versionStatus === 'Matched') return null;

        if ('DeviceOutdated' in versionStatus) {
          const { device_ver, client_ver } = versionStatus.DeviceOutdated;
          return (
            <OutdatedDeviceFirmwareModal
              deviceVer={device_ver}
              clientVer={client_ver}
              isOpen
              isProcessing={isProvisioning}
              progress={flashProgress}
              errorMessage={provisionError}
              onClose={() => !isProvisioning && setIsModalDismissed(true)}
              onUpdateFirmware={() => {
                if (wizard.context.port) {
                  void handleUpdateFirmware(wizard.context.port, 'bwm');
                }
              }}
              onCompatibilityMode={() => !isProvisioning && setIsModalDismissed(true)}
            />
          );
        }

        if ('ClientOutdated' in versionStatus) {
          const { device_ver, client_ver } = versionStatus.ClientOutdated;
          return (
            <OutdatedDesktopClientModal
              deviceVer={device_ver}
              clientVer={client_ver}
              isOpen
              onClose={() => setIsModalDismissed(true)}
              onUseLocalClient={() => setIsModalDismissed(true)}
              onRebuildClient={() => setIsModalDismissed(true)}
              onDownflash={() => setIsModalDismissed(true)}
              onContinueAnyway={() => setIsModalDismissed(true)}
            />
          );
        }

        if ('ProbeFailed' in versionStatus) {
          return (
            <VersionMismatchModal
              status={versionStatus}
              isOpen
              isProcessing={isProvisioning}
              progress={flashProgress}
              errorMessage={provisionError}
              onClose={() => setIsModalDismissed(true)}
              onUpdateFirmware={() => {
                if (wizard.context.port) {
                  void handleUpdateFirmware(wizard.context.port, 'bwm');
                }
              }}
              onCompatibilityMode={() => setIsModalDismissed(true)}
              onUseLocalClient={() => setIsModalDismissed(true)}
              onRebuildClient={() => setIsModalDismissed(true)}
              onDownflash={() => setIsModalDismissed(true)}
              onContinueAnyway={() => setIsModalDismissed(true)}
            />
          );
        }

        return null;
      })()}
    </div>
  );
}
