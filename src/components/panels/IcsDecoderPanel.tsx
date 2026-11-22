import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useWizard } from '../../hooks/useWizard';

interface Card {
  rawBits: string;
  bitCount: string;
  fc: string;
  cn: string;
  hex: string;
  blk7: string;
}

interface DiagTelemetry {
  pulses: string;
  minGapUs: string;
  maxGapUs: string;
  firstUs: string;
  lastUs: string;
  overflow: string;
  rawFrame: string;
}

export function IcsDecoderPanel() {
  const wizard = useWizard();
  const port = wizard.context.port ?? '';
  const [loading, setLoading] = useState(false);
  const [card, setCard] = useState<Card | null>(null);
  const [notice, setNotice] = useState('');
  const [icsFileStatus, setIcsFileStatus] = useState('');

  // Target provisioning state
  const [targetStatus, setTargetStatus] = useState('No target checked');
  const [writeStatus, setWriteStatus] = useState('');
  const [detecting, setDetecting] = useState(false);
  const [writing, setWriting] = useState(false);

  // BRUTE solver state
  const [brutePrintedCn, setBrutePrintedCn] = useState('');
  const [brutePrintedFc, setBrutePrintedFc] = useState('');
  const [bruteRunning, setBruteRunning] = useState(false);
  const [bruteMatchedCn, setBruteMatchedCn] = useState<{ type: string; offset: number; length: number; value: number } | null>(null);
  const [bruteMatchedFc, setBruteMatchedFc] = useState<{ type: string; offset: number; length: number; value: number } | null>(null);
  const [bruteFormatDef, setBruteFormatDef] = useState('');
  const [bruteStatus, setBruteStatus] = useState('');
  const [bruteCandidates, setBruteCandidates] = useState<Array<{ cn_start: number; cn_len: number; cn_val: number; fc_start?: number; fc_len?: number; fc_val?: number }>>([]);
  const [bruteSelectedIdx, setBruteSelectedIdx] = useState(0);

  // Signal Health & Diagnostics state
  const [diagTelemetry, setDiagTelemetry] = useState<DiagTelemetry | null>(null);
  const [iclassKey, setIclassKey] = useState('AEA684A6DAB23278');
  const [customKey, setCustomKey] = useState('');
  const [diagBanner, setDiagBanner] = useState("Click 'Fetch Telemetry' to analyze reader signal health.");
  const [fetchingTelemetry, setFetchingTelemetry] = useState(false);
  const runRd = async () => {
    if (!port) { setNotice('No port connected'); return; }
    setLoading(true);
    setNotice('');
    try {
      const res = await invoke<string>('run_raw_command', { port, command: 'hw pm5 icsd rd' });
      const m = res.match(/\$A_CARD_START\$([\s\S]*?)\$A_CARD_STOP\$/);
      if (!m) {
        if (res.includes('??')) setNotice('No card staged in reader buffer');
        else setNotice('No card block returned');
        setCard(null);
        return;
      }
      const body = m[1];
      const get = (k: string) => {
        const r = new RegExp(k + '#:\\s*(.*)');
        const mm = body.match(r);
        return mm ? mm[1].trim() : '';
      };
      const rawBits = get('wiedata') || get('Bits');
      const bitCount = get('Bit');
      const fc = get('FC');
      const cn = get('ID');
      const hex = get('Hex');
      const blk7 = get('Blk7');
      if (rawBits === '??' || !bitCount) {
        setNotice('No card staged in reader buffer');
        setCard(null);
      } else {
        setCard({ rawBits, bitCount, fc, cn, hex, blk7 });
      }
    } catch (e: any) {
      setNotice(`Error: ${String(e)}`);
      setCard(null);
    } finally {
      setLoading(false);
    }
  };

  const [copied, setCopied] = useState('');
  const runCmd = async (c: string) => {
    if (!port) { setNotice('No port connected'); return; }
    if (c.toUpperCase() === 'RD') { runRd(); return; }
    if (c.toUpperCase() === 'STATUS') { runDiag(); return; }
    setLoading(true);
    try {
      await invoke<string>('run_raw_command', { port, command: `hw pm5 icsd ${c.toLowerCase()}` });
      setNotice('');
    } catch (e: any) {
      setNotice(`Error: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  const copy = async (text: string, label: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(''), 1000);
  };

  const parseDiagFrame = (output: string): DiagTelemetry | null => {
    const m = output.match(/\$FRAME_START\$([\s\S]*?)\$FRAME_STOP\$/);
    if (!m) return null;
    const body = m[1];
    const get = (k: string) => {
      const r = new RegExp(k + '#:\\s*(.*)');
      const mm = body.match(r);
      return mm ? mm[1].trim() : '';
    };
    const pulses = get('Pulses');
    const minGapUs = get('MinGapUs');
    const maxGapUs = get('MaxGapUs');
    const firstUs = get('FirstUs');
    const lastUs = get('LastUs');
    const overflow = get('Overflow');
    if (!pulses && !minGapUs && !maxGapUs && !firstUs && !lastUs && overflow === '') {
      return null;
    }
    return { pulses, minGapUs, maxGapUs, firstUs, lastUs, overflow, rawFrame: output };
  };

  const runDiag = async () => {
    if (!port) { setNotice('No port connected'); return; }
    setLoading(true);
    setNotice('');
    try {
      const res = await invoke<string>('run_raw_command', { port, command: 'hw pm5 icsd raw STATUS' });
      const diag = parseDiagFrame(res);
      if (diag) {
        setDiagTelemetry(diag);
      }
      setNotice('');
    } catch (e: any) {
      setNotice(`Error: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  };

  const on_fetch_telemetry_clicked = async () => {
    if (!port) {
      setDiagBanner('[!] PM5 not connected.');
      return;
    }
    setFetchingTelemetry(true);
    setDiagBanner('Fetching telemetry...');
    try {
      const res = await invoke<string>('run_raw_command', { port, command: 'hw pm5 icsd raw STATUS' });
      if (!res || res.includes('NO FRAMES') || !res.includes('$FRAME_START$')) {
        setDiagBanner("[!] No signal data captured. Please present a card to the external reader connected to the iCS Decoder first, then click 'Fetch Telemetry'.");
      } else {
        const diag = parseDiagFrame(res);
        if (diag) {
          setDiagTelemetry(diag);
          setDiagBanner('[+] Signal health telemetry loaded successfully.');
        } else {
          setDiagBanner("[!] No signal data captured. Please present a card to the external reader connected to the iCS Decoder first, then click 'Fetch Telemetry'.");
        }
      }
    } catch (e: any) {
      const msg = String(e);
      if (msg.toLowerCase().includes('not connected') || msg.toLowerCase().includes('port') || msg.toLowerCase().includes('closed')) {
        setDiagBanner('[!] PM5 not connected.');
      } else {
        setDiagBanner(`[!] ${msg}`);
      }
    } finally {
      setFetchingTelemetry(false);
    }
  };

  const basename = (path: string) => path.split(/[/\\]/).pop() ?? path;

  const on_save_capture_clicked = async () => {
    if (!card) {
      setIcsFileStatus('[!] No active card data to save.');
      return;
    }
    const timestamp = Date.now();
    const defaultName = `ics_card_${card.cn}_${timestamp}.json`;
    const payload: any = {
      timestamp,
      source: 'iCS_Decoder_PM5',
      raw_bits: card.rawBits,
      bit_count: parseInt(card.bitCount, 10) || 0,
      hex: card.hex,
      facility_code: parseInt(card.fc, 10) || 0,
      card_number: parseInt(card.cn, 10) || 0,
      block7: card.blk7,
    };
    // Add timing object if diagnostic telemetry is present
    if (diagTelemetry) {
      payload.timing = {
        min_gap_us: parseInt(diagTelemetry.minGapUs, 10) || 0,
        max_gap_us: parseInt(diagTelemetry.maxGapUs, 10) || 0,
        pulses: parseInt(diagTelemetry.pulses, 10) || 0,
      };
    }
    const jsonStr = JSON.stringify(payload, null, 2);
    try {
      const result = await invoke<string>('save_ics_capture', { payloadJson: jsonStr, defaultName });
      if (result === 'cancelled') {
        setIcsFileStatus('[~] Save cancelled');
      } else {
        setIcsFileStatus(`[+] Saved: ${basename(result)}`);
      }
    } catch (e: any) {
      setIcsFileStatus(`[!] Save failed: ${String(e)}`);
      console.error('Save capture error:', e);
    }
  };

  const on_load_file_clicked = async () => {
    try {
      const rawJson = await invoke<string>('load_ics_capture');
      if (rawJson === 'cancelled') {
        setIcsFileStatus('[~] Load cancelled');
        return;
      }
      const data = JSON.parse(rawJson);
      const required = ['raw_bits', 'bit_count', 'facility_code', 'card_number', 'hex', 'block7'];
      for (const k of required) {
        if (!(k in data)) throw new Error(`Missing key: ${k}`);
      }
      setCard({
        rawBits: String(data.raw_bits),
        bitCount: String(data.bit_count),
        fc: String(data.facility_code),
        cn: String(data.card_number),
        hex: String(data.hex),
        blk7: String(data.block7),
      });
      // Load diagnostic telemetry if present in JSON
      if (data.timing) {
        setDiagTelemetry({
          pulses: String(data.timing.pulses || 0),
          minGapUs: String(data.timing.min_gap_us || 0),
          maxGapUs: String(data.timing.max_gap_us || 0),
          firstUs: '0',
          lastUs: '0',
          overflow: '0',
          rawFrame: '',
        });
      } else {
        setDiagTelemetry(null);
      }
      // Try to extract filename from the JSON or use a generic message
      const sourceInfo = data.source || 'file';
      setIcsFileStatus(`[+] Loaded: ${sourceInfo}`);
    } catch (e: any) {
      setIcsFileStatus(`[!] Load failed: ${String(e)}`);
      console.error('Load capture error:', e);
    }
  };

  const on_export_hex_clicked = async () => {
    if (!card) {
      setIcsFileStatus('[!] No active card data to export.');
      return;
    }
    const defaultName = `card_${card.cn}.hex`;
    try {
      const result = await invoke<string>('export_raw_file', {
        content: card.blk7,
        defaultName,
        extension: 'hex',
      });
      if (result === 'cancelled') {
        setIcsFileStatus('[~] Export cancelled');
      } else {
        setIcsFileStatus(`[+] Exported Hex: ${basename(result)}`);
      }
    } catch (e: any) {
      setIcsFileStatus(`[!] Export failed: ${String(e)}`);
      console.error('Export hex error:', e);
    }
  };

  const on_export_binary_clicked = async () => {
    if (!card) {
      setIcsFileStatus('[!] No active card data to export.');
      return;
    }
    const defaultName = `card_${card.cn}_raw.bin`;
    try {
      const result = await invoke<string>('export_raw_file', {
        content: card.rawBits,
        defaultName,
        extension: 'bin',
      });
      if (result === 'cancelled') {
        setIcsFileStatus('[~] Export cancelled');
      } else {
        setIcsFileStatus(`[+] Exported Binary: ${basename(result)}`);
      }
    } catch (e: any) {
      setIcsFileStatus(`[!] Export failed: ${String(e)}`);
      console.error('Export binary error:', e);
    }
  };

  const on_export_diag_clicked = async () => {
    if (!diagTelemetry) {
      setIcsFileStatus('[!] No diagnostic frame captured to export.');
      return;
    }
    const timestamp = Date.now();
    const defaultName = `diag_frame_${timestamp}.log`;
    try {
      const result = await invoke<string>('export_raw_file', {
        content: diagTelemetry.rawFrame || `Pulses: ${diagTelemetry.pulses}\nMinGapUs: ${diagTelemetry.minGapUs}\nMaxGapUs: ${diagTelemetry.maxGapUs}\nFirstUs: ${diagTelemetry.firstUs}\nLastUs: ${diagTelemetry.lastUs}\nOverflow: ${diagTelemetry.overflow}`,
        defaultName,
        extension: 'log',
      });
      if (result === 'cancelled') {
        setIcsFileStatus('[~] Export cancelled');
      } else {
        setIcsFileStatus(`[+] Exported Diag Log: ${basename(result)}`);
      }
    } catch (e: any) {
      setIcsFileStatus(`[!] Export failed: ${String(e)}`);
      console.error('Export diag error:', e);
    }
  };

  const on_detect_target_clicked = async () => {
    if (!port) {
      setTargetStatus('[!] No port connected');
      return;
    }
    setDetecting(true);
    setTargetStatus('Detecting target tag...');
    setWriteStatus('');
    let hfFound = false;
    let lfFound = false;

    // Step 1: Try HF iCLASS detection first
    try {
      const hfRes = await invoke<string>('run_raw_command', { port, command: 'hf iclass info' });
      if (hfRes && (hfRes.includes('iCLASS') || hfRes.includes('CSN') || hfRes.includes('App areas') || hfRes.includes('UID'))) {
        hfFound = true;
      }
    } catch (e) {
      // hf iclass info failed - no HF tag or command error, continue to LF
      console.debug('HF detection failed:', e);
    }

    // Step 2: Try LF search
    try {
      const lfRes = await invoke<string>('run_raw_command', { port, command: 'lf search' });
      if (lfRes && (lfRes.includes('T55') || lfRes.includes('HID') || lfRes.includes('EM410x') || lfRes.includes('LF'))) {
        lfFound = true;
      }
    } catch (e) {
      // lf search failed - no LF tag or command error
      console.debug('LF detection failed:', e);
    }

    // Step 3: Evaluate results
    if (hfFound && lfFound) {
      setTargetStatus('[+] Found both LF and HF tags on antenna');
    } else if (hfFound) {
      setTargetStatus('[+] Found iCLASS Tag (Legacy compatible) on HF coil');
    } else if (lfFound) {
      setTargetStatus('[+] Found LF T5577 / HID Tag on LF coil');
    } else {
      setTargetStatus('[!] No tag detected on PM5 antenna. Ensure blank tag is placed flat on coil.');
    }
    setDetecting(false);
  };

  const on_write_iclass_clicked = async () => {
    if (!card) {
      setWriteStatus('[!] No card data loaded');
      return;
    }
    if (!port) {
      setWriteStatus('[!] No port connected');
      return;
    }
    if (!card.blk7) {
      setWriteStatus('[!] Block 7 Hex is empty — cannot write iCLASS');
      return;
    }
    setWriting(true);
    setWriteStatus('Writing to iCLASS Legacy (HF)...');
    try {
      let selectedKey = iclassKey === 'custom' ? customKey : iclassKey;
      if (!selectedKey) selectedKey = 'AEA684A6DAB23278';
      const cmd = `hf iclass wrbl --blk 7 -d ${card.blk7} -k ${selectedKey}`;
      const res = await invoke<string>('run_raw_command', { port, command: cmd });
      setWriteStatus(`[+] Write result:\\n${res}`);
      if (!/Wrote block|ok/i.test(res)) {
        setWriteStatus(prev => `${prev}\\n[-] Write failed with key ${selectedKey}. Tag halted or rejected key.`);
        setWriting(false);
        return;
      }
      const verifyCmd = `hf iclass rdbl --blk 7 -k ${selectedKey}`;
      const verifyRes = await invoke<string>('run_raw_command', { port, command: verifyCmd });
      const m = verifyRes.match(new RegExp('block\\\\s+\\\\d+\\\\s*/0x[0-9A-Fa-f]+\\\\s*:\\\\s+([A-Fa-f0-9 ]+)'));
      const readBack = m ? m[1].replace(/\\s+/g, '').toUpperCase() : '';
      if (readBack === card.blk7.toUpperCase()) {
        setWriteStatus(prev => `${prev}\\n[+] VERIFY SUCCESS: Block 7 written and verified (${card.blk7})`);
      } else {
        setWriteStatus(prev => `${prev}\\n[-] VERIFY FAILED: Readback data did not match expected payload.`);
      }
    } catch (e: any) {
      setWriteStatus(`[!] iCLASS write failed: ${String(e)}`);
      console.error('Write iCLASS error:', e);
    } finally {
      setWriting(false);
    }
  };

  const on_write_t5577_clicked = async () => {
    if (!card) {
      setWriteStatus('[!] No card data loaded');
      return;
    }
    if (!port) {
      setWriteStatus('[!] No port connected');
      return;
    }
    const bitCount = parseInt(card.bitCount, 10) || 0;
    let cmd = '';
    if (bitCount === 26) {
      cmd = `lf hid clone -w H10301 --fc ${card.fc} --cn ${card.cn}`;
    } else if (bitCount === 37) {
      cmd = `lf hid clone -w H10302 --cn ${card.cn}`;
    } else if (bitCount > 48) {
      setWriteStatus('[!] Bit length (>48) exceeds standard HID LF cloning format; raw block writing required.');
      return;
    } else {
      setWriteStatus(`[!] Unsupported bit count ${bitCount} for standard HID LF clone.`);
      return;
    }
    setWriting(true);
    setWriteStatus(`Writing to T5577 (LF) using: ${cmd}...`);
    try {
      const res = await invoke<string>('run_raw_command', { port, command: cmd });
      setWriteStatus(`[+] T5577 write result:\n${res}`);
    } catch (e: any) {
      setWriteStatus(`[!] T5577 write failed: ${String(e)}`);
      console.error('Write T5577 error:', e);
    } finally {
      setWriting(false);
    }
  };

  const on_run_brute_clicked = async () => {
    const printedCnStr = brutePrintedCn.trim();
    if (!printedCnStr) {
      setBruteStatus('[!] Enter printed card number to match');
      return;
    }
    const printedCn = parseInt(printedCnStr, 10);
    if (isNaN(printedCn)) {
      setBruteStatus('[!] Printed Card ID must be a number');
      return;
    }
    const printedFcStr = brutePrintedFc.trim();
    const printedFc = printedFcStr ? parseInt(printedFcStr, 10) : undefined;
    if (printedFcStr && isNaN(printedFc!)) {
      setBruteStatus('[!] Printed Facility Code must be a number');
      return;
    }

    setBruteRunning(true);
    setBruteStatus('Running BRUTE analysis...');
    setBruteMatchedCn(null);
    setBruteMatchedFc(null);
    setBruteFormatDef('');
    setBruteCandidates([]);

    try {
      // Optional hardware logging
      if (port) {
        try { await invoke<string>('run_raw_command', { port, command: 'hw pm5 icsd brute' }); } catch (e) { console.debug('hw brute log failed', e); }
      }

      const bits = (card?.rawBits || '').trim();
      if (!bits || !/^[01]+$/.test(bits)) {
        setBruteStatus('[!] No valid raw bitstream captured. Read a card first.');
        setBruteRunning(false);
        return;
      }
      const N = bits.length;
      const target = BigInt(printedCn);
      const minL = target === 0n ? 1 : target.toString(2).length;

      const cands: Array<{ cn_start: number; cn_len: number; cn_val: number; fc_start?: number; fc_len?: number; fc_val?: number }> = [];

      for (let L = minL; L <= N - 2; L++) {
        for (let S = 1; S <= N - L - 1; S++) {
          const slice = bits.substring(S, S + L);
          if (BigInt('0b' + slice) !== target) continue;
          cands.push({ cn_start: S, cn_len: L, cn_val: Number(target) });
        }
      }

      for (const c of cands) {
        if (printedFc !== undefined && !isNaN(printedFc)) {
          for (let F_S = 1; F_S < c.cn_start; F_S++) {
            for (let F_L = 6; F_L <= c.cn_start - F_S; F_L++) {
              const fcVal = BigInt('0b' + bits.substring(F_S, F_S + F_L));
              if (fcVal === BigInt(printedFc)) {
                c.fc_start = F_S;
                c.fc_len = F_L;
                c.fc_val = Number(fcVal);
              }
            }
          }
        } else {
          for (const w of [8, 12, 16, 20, 22]) {
            if (w < c.cn_start) {
              const fs = c.cn_start - w;
              if (fs >= 1) {
                const fcVal = parseInt(bits.substring(fs, fs + w), 2);
                c.fc_start = fs;
                c.fc_len = w;
                c.fc_val = fcVal;
                break;
              }
            }
          }
        }
      }

      // Keep all valid candidates (do not dedup by end)
      let deduped = cands;

      // Prioritize exact dual matches when printedFc provided
      if (printedFc !== undefined && !isNaN(printedFc)) {
        const exact = deduped.filter(c => c.fc_val === printedFc);
        const rest = deduped.filter(c => c.fc_val !== printedFc);
        deduped = [...exact, ...rest];
      } else {
        deduped.sort((a, b) => b.cn_len - a.cn_len);
      }

      setBruteCandidates(deduped);
      setBruteSelectedIdx(0);
      if (deduped.length === 0) {
        setBruteStatus('[!] No Card ID match found in bitstream');
        setBruteMatchedCn(null);
        setBruteMatchedFc(null);
        setBruteFormatDef('');
      } else {
        const c = deduped[0];
        setBruteMatchedCn({ type: `CN${c.cn_len}`, offset: c.cn_start, length: c.cn_len, value: c.cn_val });
        if (c.fc_start !== undefined) {
          setBruteMatchedFc({ type: `FC${c.fc_len}`, offset: c.fc_start, length: c.fc_len!, value: c.fc_val! });
        } else {
          setBruteMatchedFc(null);
        }
        let formatParts: string[] = [];
        if (c.fc_start !== undefined) {
          formatParts.push(`FC: start=${c.fc_start}, len=${c.fc_len} (${c.fc_val})`);
        } else {
          formatParts.push('FC: unknown');
        }
        formatParts.push(`CN: start=${c.cn_start}, len=${c.cn_len} (${c.cn_val})`);
        setBruteFormatDef(`Format: [Total: ${N}b | ${formatParts.join(' | ')}]`);
        setBruteStatus(`[+] ${deduped.length} candidate(s) found. Select below.`);
      }
    } catch (e: any) {
      setBruteStatus(`[!] BRUTE error: ${String(e)}`);
      console.error('BRUTE error:', e);
    } finally {
      setBruteRunning(false);
    }
  };

  const on_copy_format_def = async () => {
    if (!bruteFormatDef) return;
    await navigator.clipboard.writeText(bruteFormatDef);
    setBruteStatus('[+] Format definition copied to clipboard');
    setTimeout(() => {
      if (bruteStatus.includes('copied')) setBruteStatus(bruteMatchedCn ? '[+] Format resolved' : '');
    }, 1500);
  };

  return (
    <div style={{ padding: '24px', position: 'relative', zIndex: 5 }}>
      <h3>ICS DECODER</h3>
      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 16 }}>
        {['Who','RD','STATUS','CLEAR','BITS','BRUTE','HELP'].map(c => (
          <button key={c} onClick={() => runCmd(c)} disabled={loading || !port}>{c}</button>
        ))}
      </div>

      {notice && <div style={{ color: 'var(--amber-bright)', marginBottom: 12 }}>{notice}</div>}

      {card && (
        <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, maxWidth: 520 }}>
          <h4>Decoded Credential Card</h4>
          <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
            <button onClick={on_save_capture_clicked}>Save Capture</button>
            <button onClick={on_load_file_clicked}>Load File</button>
          </div>
          {icsFileStatus && (
            <div
              id="ics_file_status"
              style={{
                marginBottom: 12,
                color: icsFileStatus.startsWith('[!]') ? 'var(--red-bright)' : 'var(--green-bright)',
              }}
            >
              {icsFileStatus}
            </div>
          )}
          <div style={{ fontSize: 24, fontWeight: 700, marginBottom: 8 }}>Card ID: {card.cn}</div>
          <div>Facility Code: {card.fc}</div>
          <div>Format / Bit Count: {card.bitCount}-bit</div>
          <div>Block 7 Hex: {card.blk7}</div>
          <div style={{ marginTop: 8 }}>
            <label>Raw Binary Bitstream</label>
            <input readOnly value={card.rawBits} style={{ width: '100%', fontFamily: 'monospace' }} />
          </div>
          <div style={{ marginTop: 8, display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <button onClick={() => copy(card.hex, 'hex')}>{copied === 'hex' ? 'Copied!' : 'Copy Hex'}</button>
            <button onClick={() => copy(card.rawBits, 'bin')}>{copied === 'bin' ? 'Copied!' : 'Copy Binary'}</button>
            <button onClick={on_export_hex_clicked} disabled={!card}>Export Hex File</button>
            <button onClick={on_export_binary_clicked} disabled={!card}>Export Binary File</button>
          </div>
        </div>
      )}

      {/* Signal Health & Diagnostics Panel */}
      {card && (
        <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, maxWidth: 520, marginTop: 16 }}>
          <h4>Signal Health & Diagnostics</h4>
          <div
            style={{
              marginBottom: 12,
              padding: 8,
              background: 'var(--bg-surface)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              fontFamily: 'monospace',
              fontSize: 12,
              color: diagBanner.startsWith('[!]') ? 'var(--amber-bright)' :
                     diagBanner.startsWith('[+]') ? 'var(--green-bright)' :
                     'var(--text-primary)',
            }}
          >
            {diagBanner}
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 12 }}>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
              }}
            >
              <span>Pulses Recorded</span>
              <strong>{diagTelemetry?.pulses ? `${diagTelemetry.pulses} edges` : '—'}</strong>
            </div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
              }}
            >
              <span>Pulse Gap Timing</span>
              <strong>{diagTelemetry?.minGapUs && diagTelemetry?.maxGapUs
                ? `Min: ${diagTelemetry.minGapUs} µs | Max: ${diagTelemetry.maxGapUs} µs`
                : 'Min: — µs | Max: — µs'}</strong>
            </div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
              }}
            >
              <span>Frame Duration</span>
              <strong>{diagTelemetry?.firstUs && diagTelemetry?.lastUs
                ? `${diagTelemetry.firstUs} µs to ${diagTelemetry.lastUs} µs`
                : '— µs to — µs'}</strong>
            </div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
              }}
            >
              <span>Buffer Integrity</span>
              <strong style={{ color: diagTelemetry?.overflow === '1' ? 'var(--red-bright)' : 'var(--green-bright)' }}>
                {diagTelemetry?.overflow === '1' ? '[!] Buffer Overflow' : 'Normal (No Overflow)'}
              </strong>
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <button
              onClick={on_fetch_telemetry_clicked}
              disabled={fetchingTelemetry || !port}
            >
              {fetchingTelemetry ? 'Fetching...' : 'Fetch Telemetry'}
            </button>
            <button
              onClick={on_export_diag_clicked}
              disabled={!diagTelemetry}
              style={{ opacity: diagTelemetry ? 1 : 0.5 }}
            >
              Export Diag Log
            </button>
          </div>
        </div>
      )}

      {/* Provision Target Card Panel */}
      {card && (
        <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, maxWidth: 520, marginTop: 16 }}>
          <h4>Provision Target Card</h4>
          {/* Dynamic Target Guidance */}
          {(() => {
            const bitCount = parseInt(card.bitCount, 10) || 0;
            if (bitCount > 37) {
              return (
                <div
                  style={{
                    marginBottom: 12,
                    padding: 8,
                    background: 'var(--bg-highlight)',
                    border: '1px solid var(--amber)',
                    borderRadius: 4,
                    fontFamily: 'monospace',
                    fontSize: 11,
                    color: 'var(--amber)',
                  }}
                >
                  Target Requirement: [HF ONLY] This credential is {bitCount}-bit and requires an iCLASS Legacy 2k/2 blank card on the PM5 coil.
                </div>
              );
            } else {
              return (
                <div
                  style={{
                    marginBottom: 12,
                    padding: 8,
                    background: 'var(--bg-highlight)',
                    border: '1px solid var(--green-mid)',
                    borderRadius: 4,
                    fontFamily: 'monospace',
                    fontSize: 11,
                    color: 'var(--green-mid)',
                  }}
                >
                  Target Requirement: [LF or HF] Can be cloned to LF T5577 or written to iCLASS Legacy.
                </div>
              );
            }
          })()}
          <div style={{ display: 'flex', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
            <button onClick={on_detect_target_clicked} disabled={detecting || !port}>
              {detecting ? 'Detecting...' : 'Detect Target Tag'}
            </button>
          </div>
          <div
            id="target_detected_status"
            style={{
              marginBottom: 12,
              padding: 8,
              background: 'var(--bg-surface)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              fontFamily: 'monospace',
              fontSize: 12,
              minHeight: 20,
              color: targetStatus.startsWith('[!]') ? 'var(--red-bright)' :
                     targetStatus.startsWith('[+]') ? 'var(--green-bright)' :
                     'var(--amber)',
            }}
          >
            {targetStatus}
          </div>
          <div style={{ marginBottom: 12, padding: 8, background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 4, fontFamily: 'monospace', fontSize: 11, color: 'var(--green-bright)' }}>
            Ready to write solved payload to iCLASS Legacy Block 7 (Application Area 1).
          </div>
          <div style={{ display: 'flex', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
            <button
              onClick={on_write_iclass_clicked}
              disabled={writing || !port || !card.blk7}
              style={{ opacity: card.blk7 ? 1 : 0.5 }}
            >
              {writing ? 'Writing...' : 'Write to iCLASS Legacy (HF)'}
            </button>
            <button
              onClick={on_write_t5577_clicked}
              disabled={writing || !port || (parseInt(card.bitCount, 10) || 0) > 37}
              style={{ opacity: ((parseInt(card.bitCount, 10) || 0) > 37) ? 0.5 : 1 }}
              title={(parseInt(card.bitCount, 10) || 0) > 37 ? 'LF T5577 does not support standard >37/48-bit HID formats' : ''}
            >
              {writing ? 'Writing...' : 'Write to T5577 (LF)'}
            </button>
          </div>
          <div
            id="write_status_output"
            style={{
              marginTop: 8,
              padding: 8,
              background: 'var(--bg-surface)',
              border: '1px solid var(--border)',
              borderRadius: 4,
              fontFamily: 'monospace',
              fontSize: 11,
              minHeight: 60,
              maxHeight: 200,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              color: writeStatus.startsWith('[!]') ? 'var(--red-bright)' :
                     writeStatus.startsWith('[+]') ? 'var(--green-bright)' :
                     'var(--text-primary)',
            }}
          >
            {writeStatus || 'Write status output...'}
          </div>
        </div>
      )}

      {/* Format Reverse-Engineering (BRUTE Solver) Panel */}
      {card && (
        <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, maxWidth: 520, marginTop: 16 }}>
          <h4>Format Reverse-Engineering (BRUTE Solver)</h4>
          <div style={{ display: 'flex', gap: 8, marginBottom: 12, flexWrap: 'wrap', alignItems: 'flex-end' }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4, minWidth: 180 }}>
              <label style={{ fontSize: 11, color: 'var(--text-secondary)' }}>Printed Card ID</label>
              <input
                type="text"
                placeholder="e.g., 2493837"
                value={brutePrintedCn}
                onChange={e => setBrutePrintedCn(e.target.value)}
                style={{
                  padding: '4px 8px',
                  background: 'var(--bg-surface)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: 'var(--text-primary)',
                  fontFamily: 'monospace',
                  fontSize: 13,
                }}
              />
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4, minWidth: 150 }}>
              <label style={{ fontSize: 11, color: 'var(--text-secondary)' }}>Printed Facility Code (opt)</label>
              <input
                type="text"
                placeholder="e.g., 890"
                value={brutePrintedFc}
                onChange={e => setBrutePrintedFc(e.target.value)}
                style={{
                  padding: '4px 8px',
                  background: 'var(--bg-surface)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  color: 'var(--text-primary)',
                  fontFamily: 'monospace',
                  fontSize: 13,
                }}
              />
            </div>
            <button
              onClick={on_run_brute_clicked}
              disabled={bruteRunning || (!port && !card)}
              style={{ padding: '4px 12px', height: 'fit-content' }}
            >
              {bruteRunning ? 'Analyzing...' : 'Run BRUTE Analysis'}
            </button>
          </div>
          {bruteCandidates.length > 0 && (
            <select
              value={bruteSelectedIdx}
              onChange={e => {
                const idx = parseInt(e.target.value, 10);
                setBruteSelectedIdx(idx);
                const c = bruteCandidates[idx];
                if (!c) return;
                const fc2p = c.cn_start > 2 ? parseInt((card?.rawBits || '').substring(2, c.cn_start), 2) || 0 : 0;
                setBruteMatchedCn({ type: `CN${c.cn_len}`, offset: c.cn_start, length: c.cn_len, value: c.cn_val });
                setBruteMatchedFc({ type: `FC${c.cn_start - 2}`, offset: 2, length: c.cn_start - 2, value: fc2p });
                setBruteFormatDef(`Format: [Total=${(card?.rawBits || '').length}b | FC: start=2, len=${c.cn_start - 2} (${fc2p}) | CN: start=${c.cn_start}, len=${c.cn_len} (${c.cn_val})]`);
              }}
              style={{ width: '100%', padding: '4px 8px', background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 4, color: 'var(--text-primary)', fontFamily: 'monospace', fontSize: 12, marginBottom: 12 }}
            >
              {bruteCandidates.map((c, i) => {
                const fc2p = c.cn_start > 2 ? parseInt((card?.rawBits || '').substring(2, c.cn_start), 2) || 0 : 0;
                const fc1p = c.cn_start > 1 ? parseInt((card?.rawBits || '').substring(1, c.cn_start), 2) || 0 : 0;
                return (
                  <option key={i} value={i}>
                    Candidate {i + 1}: CN @ offset {c.cn_start} (len {c.cn_len}, val {c.cn_val}) | FC (2p): {fc2p} | FC (1p): {fc1p}
                  </option>
                );
              })}
            </select>
          )}
          {bruteStatus && (
            <div
              style={{
                marginBottom: 12,
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
                color: bruteStatus.startsWith('[!]') ? 'var(--red-bright)' :
                       bruteStatus.startsWith('[+]') ? 'var(--green-bright)' :
                       bruteStatus.startsWith('[~]') ? 'var(--amber)' :
                       'var(--text-primary)',
              }}
            >
              {bruteStatus}
            </div>
          )}
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 12 }}>
            <div
              style={{
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
                minHeight: 40,
              }}
            >
              <div><strong>Matched Card ID:</strong> {bruteMatchedCn
                ? `Found ${bruteMatchedCn.type} @ offset ${bruteMatchedCn.offset} (Decimal: ${bruteMatchedCn.value}, Len: ${bruteMatchedCn.length}b)`
                : '—'}</div>
            </div>
            <div
              style={{
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
                minHeight: 40,
              }}
            >
              <div><strong>Matched Facility Code:</strong> {bruteMatchedFc
                ? `Found ${bruteMatchedFc.type} @ offset ${bruteMatchedFc.offset} (Decimal: ${bruteMatchedFc.value}, Len: ${bruteMatchedFc.length}b)`
                : brutePrintedFc ? 'No match' : '— (optional)'}</div>
            </div>
            <div
              style={{
                padding: 8,
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 4,
                fontFamily: 'monospace',
                fontSize: 12,
                minHeight: 40,
              }}
            >
              <div><strong>Format Definition:</strong> {bruteFormatDef || '—'}</div>
            </div>
          </div>
          <div style={{ display: 'flex', gap: '8px', marginTop: '12px', alignItems: 'center' }}>
            <select value={iclassKey} onChange={e => setIclassKey(e.target.value)} style={{ padding: '4px 8px', background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 4, color: 'var(--text-primary)', fontFamily: 'monospace', fontSize: 12 }}>
              <option value="AEA684A6DAB23278">HID Standard Master (AEA684A6DAB23278)</option>
              <option value="2020666666668888">iCopy-X DRM Master (2020666666668888)</option>
              <option value="custom">Custom Key</option>
            </select>
            {iclassKey === 'custom' && (
              <input value={customKey} onChange={e => setCustomKey(e.target.value)} placeholder="16 hex chars" style={{ padding: '4px 8px', background: 'var(--bg-surface)', border: '1px solid var(--border)', borderRadius: 4, color: 'var(--text-primary)', fontFamily: 'monospace', fontSize: 12 }} />
            )}
            {bruteFormatDef && (
              <button onClick={on_copy_format_def} style={{ padding: '4px 12px' }}>
                Copy Format Def
              </button>
            )}
            <button
              onClick={on_write_iclass_clicked}
              disabled={!card || !card.blk7 || !port}
              style={{ padding: '4px 12px' }}
            >
              Write Solved to iCLASS Legacy (HF)
            </button>
          </div>
        </div>
      )}
    </div>
  );
}