use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Frequency {
    LF,
    HF,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types)]
pub enum CardType {
    // LF cloneable types (22 total)
    EM4100,
    HIDProx,
    Indala,
    IOProx,
    AWID,
    FDX_B,
    Paradox,
    Viking,
    Pyramid,
    Keri,
    NexWatch,
    Presco,
    Nedap,
    GProxII,
    Gallagher,
    PAC,
    Noralsy,
    Jablotron,
    SecuraKey,
    Visa2000,
    Motorola,
    IDTECK,
    // LF non-cloneable types (3)
    COTAG,
    EM4x50,
    EM4305,
    Hitag,
    /// Hitag2 (distinct from generic Hitag). Supports Paxton Net2/Switch2 probes.
    Hitag2,
    /// Paxton Net2 (Hitag2-derived).
    PaxtonNet2,
    /// Paxton Switch2 (Hitag2-derived).
    PaxtonSwitch2,
    /// Topaz / Jewel (NFC).
    Topaz,
    /// ISO 14443-B.
    ISO14443B,
    /// ISO 15693 ICODE.
    ISO15693ICODE,
    /// ISO 15693 ST.
    ISO15693ST,
    /// LEGIC.
    Legic,
    /// FeliCa.
    Felica,
    /// Generic T55xx chipset (T5577/T5555) reported by `lf search` as
    /// "[+] Chipset... T55xx". Not a cloneable type -- these are config chips,
    /// handled by the T55xx Advanced tab rather than the clone wizard.
    T55xx,
    // HF types
    MifareClassic1K,
    MifareClassic4K,
    MifareUltralight,
    NTAG,
    DESFire,
    IClass,
    // MIFARE Classic variants (Phase 2 subtype resolution)
    MifareClassicMini,
    MifareClassicPlus2K,
    MifareClassicPossible,
    // Ultralight / NTAG variants
    MifareUltralightC,
    MifareUltralightEV1,
    NTAG213,
    NTAG215,
    NTAG216,
    // iCLASS variants
    IClassElite,
    IClassSE,
}

impl CardType {
    pub fn frequency(&self) -> Frequency {
        match self {
            CardType::EM4100
            | CardType::HIDProx
            | CardType::Indala
            | CardType::IOProx
            | CardType::AWID
            | CardType::FDX_B
            | CardType::Paradox
            | CardType::Viking
            | CardType::Pyramid
            | CardType::Keri
            | CardType::NexWatch
            | CardType::Presco
            | CardType::Nedap
            | CardType::GProxII
            | CardType::Gallagher
            | CardType::PAC
            | CardType::Noralsy
            | CardType::Jablotron
            | CardType::SecuraKey
            | CardType::Visa2000
            | CardType::Motorola
            | CardType::IDTECK
            | CardType::COTAG
            | CardType::EM4x50
            | CardType::EM4305
            | CardType::Hitag
            | CardType::Hitag2
            | CardType::PaxtonNet2
            | CardType::PaxtonSwitch2
            | CardType::Topaz
            | CardType::ISO14443B
            | CardType::ISO15693ICODE
            | CardType::ISO15693ST
            | CardType::Legic
            | CardType::Felica
            | CardType::T55xx => Frequency::LF,

            CardType::MifareClassic1K
            | CardType::MifareClassic4K
            | CardType::MifareUltralight
            | CardType::NTAG
            | CardType::DESFire
            | CardType::IClass
            | CardType::MifareClassicMini
            | CardType::MifareClassicPlus2K
            | CardType::MifareClassicPossible
            | CardType::MifareUltralightC
            | CardType::MifareUltralightEV1
            | CardType::NTAG213
            | CardType::NTAG215
            | CardType::NTAG216
            | CardType::IClassElite
            | CardType::IClassSE => Frequency::HF,
        }
    }

    /// Human-readable name for UI display. Used by frontend via serde serialization.
    #[allow(dead_code)]
    pub fn display_name(&self) -> &str {
        match self {
            CardType::EM4100 => "EM4100",
            CardType::HIDProx => "HID Prox",
            CardType::Indala => "Indala",
            CardType::IOProx => "IO Prox",
            CardType::AWID => "AWID",
            CardType::FDX_B => "FDX-B",
            CardType::Paradox => "Paradox",
            CardType::Viking => "Viking",
            CardType::Pyramid => "Pyramid",
            CardType::Keri => "Keri",
            CardType::NexWatch => "NexWatch",
            CardType::Presco => "Presco",
            CardType::Nedap => "Nedap",
            CardType::GProxII => "GProx II",
            CardType::Gallagher => "Gallagher",
            CardType::PAC => "PAC/Stanley",
            CardType::Noralsy => "Noralsy",
            CardType::Jablotron => "Jablotron",
            CardType::SecuraKey => "SecuraKey",
            CardType::Visa2000 => "Visa2000",
            CardType::Motorola => "Motorola",
            CardType::IDTECK => "IDTECK",
            CardType::COTAG => "COTAG",
            CardType::EM4x50 => "EM4x50",
            CardType::EM4305 => "EM4305",
            CardType::Hitag => "Hitag",
            CardType::Hitag2 => "Hitag2",
            CardType::PaxtonNet2 => "Paxton Net2",
            CardType::PaxtonSwitch2 => "Paxton Switch2",
            CardType::Topaz => "Topaz",
            CardType::ISO14443B => "ISO 14443-B",
            CardType::ISO15693ICODE => "ISO 15693 ICODE",
            CardType::ISO15693ST => "ISO 15693 ST",
            CardType::Legic => "LEGIC",
            CardType::Felica => "FeliCa",
            CardType::T55xx => "T55xx",
            CardType::MifareClassic1K => "MIFARE Classic 1K",
            CardType::MifareClassic4K => "MIFARE Classic 4K",
            CardType::MifareUltralight => "MIFARE Ultralight",
            CardType::NTAG => "NTAG",
            CardType::DESFire => "DESFire",
            CardType::IClass => "iCLASS",
            CardType::MifareClassicMini => "MIFARE Classic Mini",
            CardType::MifareClassicPlus2K => "MIFARE Classic Plus 2K",
            CardType::MifareClassicPossible => "MIFARE Classic Possible",
            CardType::MifareUltralightC => "MIFARE Ultralight C",
            CardType::MifareUltralightEV1 => "MIFARE Ultralight EV1",
            CardType::NTAG213 => "NTAG 213",
            CardType::NTAG215 => "NTAG 215",
            CardType::NTAG216 => "NTAG 216",
            CardType::IClassElite => "iCLASS Elite",
            CardType::IClassSE => "iCLASS SE",
        }
    }

    pub fn is_cloneable(&self) -> bool {
        match self {
            CardType::DESFire => false,
            CardType::COTAG => false,
            CardType::EM4x50 => false,
            CardType::Hitag => false,
            // A T55xx is a real, writable config chip. `lf search` reports the
            // generic chipset, and the source-card workflow (read blocks 0-7
            // from the source, write them to a T5577) is exactly how these are
            // cloned. Marking it non-cloneable left the user with a detected
            // card and no way to write it.
            _ => true,
        }
    }

    /// Reason why a card type cannot be cloned, if applicable.
    /// Provided for frontend display on non-cloneable card identification.
    #[allow(dead_code)]
    pub fn non_cloneable_reason(&self) -> Option<&str> {
        match self {
            CardType::DESFire => Some("DESFire uses AES encryption; cloning not supported"),
            CardType::COTAG => Some("Read-only, no clone commands available"),
            CardType::EM4x50 => Some("Requires native EM4x50 blank, not T5577-compatible"),
            CardType::Hitag => Some("Requires native Hitag chip, not T5577-compatible"),
            _ => None,
        }
    }

    /// Whether this card type supports cloning to EM4305 blanks (via `--em` flag).
    /// Only the original 11 LF types support this flag in Proxmark3 Iceman firmware.
    /// The 11 newer types (Presco, Nedap, GProxII, Gallagher, PAC, Noralsy,
    /// Jablotron, SecuraKey, Visa2000, Motorola, IDTECK) do not support `--em`
    /// and will fail silently or error if the flag is passed.
    pub fn supports_em4305(&self) -> bool {
        matches!(
            self,
            CardType::EM4100
            | CardType::HIDProx
            | CardType::Indala
            | CardType::IOProx
            | CardType::AWID
            | CardType::FDX_B
            | CardType::Paradox
            | CardType::Viking
            | CardType::Pyramid
            | CardType::Keri
            | CardType::NexWatch
        )
    }

    pub fn recommended_blank(&self) -> BlankType {
        match self {
            // All LF cloneable types use T5577 by default
            CardType::EM4100
            | CardType::HIDProx
            | CardType::Indala
            | CardType::IOProx
            | CardType::AWID
            | CardType::FDX_B
            | CardType::Paradox
            | CardType::Viking
            | CardType::Pyramid
            | CardType::Keri
            | CardType::NexWatch
            | CardType::Presco
            | CardType::Nedap
            | CardType::GProxII
            | CardType::Gallagher
            | CardType::PAC
            | CardType::Noralsy
            | CardType::Jablotron
            | CardType::SecuraKey
            | CardType::Visa2000
            | CardType::Motorola
            | CardType::IDTECK => BlankType::T5577,
            // Non-cloneable LF: return T5577 as placeholder (won't actually be used)
            CardType::COTAG | CardType::EM4x50 | CardType::EM4305 | CardType::Hitag | CardType::Hitag2
            | CardType::PaxtonNet2 | CardType::PaxtonSwitch2 | CardType::Topaz
            | CardType::ISO14443B | CardType::ISO15693ICODE | CardType::ISO15693ST
            | CardType::Legic | CardType::Felica => BlankType::T5577,
            CardType::T55xx => BlankType::T5577,
            // HF types
            CardType::MifareClassic1K | CardType::MifareClassic4K
            | CardType::MifareClassicMini | CardType::MifareClassicPlus2K
            | CardType::MifareClassicPossible => BlankType::MagicMifareGen1a,
            CardType::MifareUltralight | CardType::MifareUltralightC
            | CardType::MifareUltralightEV1 | CardType::NTAG | CardType::NTAG213
            | CardType::NTAG215 | CardType::NTAG216 => BlankType::MagicUltralight,
            CardType::DESFire => BlankType::MagicMifareGen4GTU,
            CardType::IClass | CardType::IClassElite | CardType::IClassSE => BlankType::IClassBlank,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BlankType {
    T5577,
    EM4305,
    MagicMifareGen1a,
    MagicMifareGen2,
    MagicMifareGen3,
    MagicMifareGen4GTU,
    MagicMifareGen4GDM,
    MagicUltralight,
    IClassBlank,
}

impl BlankType {
    /// Human-readable name for UI display. Used by frontend via serde serialization.
    #[allow(dead_code)]
    pub fn display_name(&self) -> &str {
        match self {
            BlankType::T5577 => "T5577",
            BlankType::EM4305 => "EM4305",
            BlankType::MagicMifareGen1a => "Magic MIFARE Gen1a",
            BlankType::MagicMifareGen2 => "Magic MIFARE Gen2 (CUID)",
            BlankType::MagicMifareGen3 => "Magic MIFARE Gen3 (UFUID)",
            BlankType::MagicMifareGen4GTU => "Magic MIFARE Gen4 GTU",
            BlankType::MagicMifareGen4GDM => "Magic MIFARE Gen4 GDM",
            BlankType::MagicUltralight => "Magic Ultralight",
            BlankType::IClassBlank => "iCLASS Blank",
        }
    }
}

/// T5577 chip detection result from `lf t55xx detect`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardCapabilities {
    pub can_read: bool,
    pub can_write: bool,
    pub can_emulate: bool,
    pub recommended_blank: BlankType,
    pub read_commands: Vec<String>,
    pub write_commands: Vec<String>,
    pub verify_commands: Vec<String>,
    pub sim_commands: Vec<String>,
    pub read_reason: Option<String>,
    pub write_reason: Option<String>,
    pub emulate_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T5577Status {
    pub detected: bool,
    pub chip_type: String,
    pub password_set: bool,
    pub block0: Option<String>,
    pub modulation: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CardData {
    pub uid: String,
    pub raw: String,
    pub decoded: HashMap<String, String>,
    // Phase 0 scan-result extensions (all optional/defaulted for backward compat)
    #[serde(default)]
    pub found: bool,
    #[serde(rename = "type", default)]
    pub card_type: Option<CardType>,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub can_read: bool,
    #[serde(default)]
    pub can_write: bool,
    #[serde(default)]
    pub can_emulate: bool,
    #[serde(default)]
    pub recommended_blank: Option<BlankType>,
    #[serde(default)]
    pub dump_file: Option<String>,
    #[serde(default)]
    pub keys_found: u32,
    #[serde(default)]
    pub read_error: Option<String>,
    #[serde(default)]
    pub non_cloneable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardSummary {
    pub card_type: String,
    pub uid: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RecoveryAction {
    Retry,
    GoBack,
    Reconnect,
    Manual,
}

/// HF card processing phases for autopwn progress tracking.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ProcessPhase {
    KeyCheck,
    Darkside,
    Nested,
    Hardnested,
    StaticNested,
    Dumping,
}

/// Events parsed line-by-line from `hf mf autopwn` streaming output.
#[derive(Clone, Debug, PartialEq)]
pub enum AutopwnEvent {
    /// Dictionary attack progress: found N of M keys, method char (D=dict)
    DictionaryProgress { found: u32, total: u32 },
    /// Individual key recovered
    KeyFound { key: String },
    /// Darkside attack started (0 known keys + WEAK PRNG)
    DarksideStarted,
    /// Nested attack started (1+ keys + WEAK PRNG)
    NestedStarted,
    /// Hardnested attack started (1+ keys + HARD PRNG)
    HardnestedStarted,
    /// Staticnested attack started (1+ keys + STATIC PRNG)
    StaticnestedStarted,
    /// All blocks dumped successfully
    DumpComplete { file_path: String },
    /// Partial dump (some sectors unreadable)
    DumpPartial { file_path: String },
    /// All key recovery attempts failed
    Failed { reason: String },
    /// Autopwn finished with execution time
    Finished { time_secs: u32 },
}

/// Magic card generation identifiers (reserved for Phase 3: HF card support).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[allow(dead_code)]
pub enum MagicGeneration {
    Gen1a,
    Gen2,
    Gen3,
    Gen4GTU,
    Gen4GDM,
}
