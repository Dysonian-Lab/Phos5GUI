use crate::cards::types::{CardCapabilities, CardType};

impl CardType {
    pub fn capabilities(&self) -> CardCapabilities {
        let rb = self.recommended_blank();
        match self {
            // Static LF cloneable types
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
            | CardType::IDTECK => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["lf search".to_string()],
                write_commands: vec![],
                verify_commands: vec!["lf search".to_string()],
                sim_commands: vec![],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            // LF read-only / non-cloneable
            CardType::COTAG | CardType::EM4x50 => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec![],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec![],
                read_reason: None,
                write_reason: Some("Read-only, no clone commands available".to_string()),
                emulate_reason: Some("No sim command available".to_string()),
            },
            CardType::Hitag => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec![],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec![],
                read_reason: None,
                write_reason: Some("Requires native Hitag chip, not T5577-compatible".to_string()),
                emulate_reason: Some("No sim command available".to_string()),
            },
            CardType::Hitag2 | CardType::PaxtonNet2 | CardType::PaxtonSwitch2 => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["lf hitag read --ht2".to_string()],
                write_commands: vec!["lf hitag wrbl --ht2".to_string()],
                verify_commands: vec!["lf hitag read --ht2".to_string()],
                sim_commands: vec!["lf hitag sim".to_string()],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            CardType::T55xx => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec!["lf t55xx dump".to_string()],
                write_commands: vec!["lf t55xx restore".to_string()],
                verify_commands: vec!["lf t55xx dump".to_string()],
                sim_commands: vec![],
                read_reason: None,
                write_reason: None,
                emulate_reason: Some("lf t55xx sim does not exist in the client".to_string()),
            },
            // EM4305
            CardType::EM4305 => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec!["lf em 4x05 dump".to_string()],
                write_commands: vec!["lf em 4x05 write".to_string()],
                verify_commands: vec!["lf em 4x05 dump".to_string()],
                sim_commands: vec![],
                read_reason: None,
                write_reason: None,
                emulate_reason: Some("lf em 4x05 sim does not exist in the client".to_string()),
            },
            // EM4305
            CardType::Felica => CardCapabilities {
                // Felica handled specially; keep conservative
                can_read: true,
                can_write: false,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf felica dump".to_string()],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec!["hf felica sim".to_string()],
                read_reason: None,
                write_reason: Some("Write not supported for FeliCa".to_string()),
                emulate_reason: None,
            },
            // Topaz / ISO14443B / ISO15693 / Legic / Topaz
            CardType::Topaz => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec![],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec![],
                read_reason: None,
                write_reason: Some("Read-only".to_string()),
                emulate_reason: Some("No sim command available".to_string()),
            },
            CardType::ISO14443B => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec![],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec!["hf 14b sim".to_string()],
                read_reason: None,
                write_reason: Some("Read-only".to_string()),
                emulate_reason: None,
            },
            CardType::ISO15693ICODE | CardType::ISO15693ST => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf 15 dump".to_string()],
                write_commands: vec!["hf 15 restore".to_string()],
                verify_commands: vec!["hf 15 dump".to_string()],
                sim_commands: vec!["hf 15 sim".to_string()],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            CardType::Legic => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf legic dump".to_string()],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec!["hf legic sim".to_string()],
                read_reason: None,
                write_reason: Some("Read-only".to_string()),
                emulate_reason: None,
            },
            // MIFARE Classic
            CardType::MifareClassic1K
            | CardType::MifareClassic4K
            | CardType::MifareClassicMini
            | CardType::MifareClassicPlus2K
            | CardType::MifareClassicPossible => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf mf autopwn".to_string()],
                write_commands: vec!["hf mf cload".to_string()],
                verify_commands: vec!["hf mf dump".to_string()],
                sim_commands: vec!["hf mf sim".to_string()],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            // Ultralight / NTAG
            CardType::MifareUltralight
            | CardType::MifareUltralightC
            | CardType::MifareUltralightEV1
            | CardType::NTAG
            | CardType::NTAG213
            | CardType::NTAG215
            | CardType::NTAG216 => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf mfu dump".to_string()],
                write_commands: vec!["hf mfu restore".to_string()],
                verify_commands: vec!["hf mfu dump".to_string()],
                sim_commands: vec!["hf mfu sim".to_string()],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            CardType::DESFire => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec!["hf mfdes info".to_string()],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec![],
                read_reason: None,
                write_reason: Some(
                    "DESFire uses AES encryption; cloning not supported".to_string(),
                ),
                emulate_reason: Some("No sim command available".to_string()),
            },
            // iCLASS
            CardType::IClass | CardType::IClassElite => CardCapabilities {
                can_read: true,
                can_write: true,
                can_emulate: true,
                recommended_blank: rb,
                read_commands: vec!["hf iclass dump".to_string()],
                write_commands: vec!["hf iclass wrbl".to_string()],
                verify_commands: vec!["hf iclass rdbl".to_string()],
                sim_commands: vec!["hf iclass sim".to_string()],
                read_reason: None,
                write_reason: None,
                emulate_reason: None,
            },
            CardType::IClassSE => CardCapabilities {
                can_read: true,
                can_write: false,
                can_emulate: false,
                recommended_blank: rb,
                read_commands: vec!["hf iclass dump".to_string()],
                write_commands: vec![],
                verify_commands: vec![],
                sim_commands: vec![],
                read_reason: None,
                write_reason: Some("iCLASS SE is read-only".to_string()),
                emulate_reason: Some("No sim command available".to_string()),
            },
        }
    }
}
