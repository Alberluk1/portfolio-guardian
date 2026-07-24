pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

#[derive(Debug, Clone, PartialEq)]
pub struct MintCore {
    pub mint_authority: Option<[u8; 32]>,
    pub supply: u64,
    pub decimals: u8,
    pub is_initialized: bool,
    pub freeze_authority: Option<[u8; 32]>,
}

const MINT_LEN: usize = 82;

fn read_optional_key(tag: &[u8], key: &[u8]) -> Option<[u8; 32]> {
    if u32::from_le_bytes(tag.try_into().ok()?) == 1 {
        let mut out = [0u8; 32];
        out.copy_from_slice(key);
        Some(out)
    } else {
        None
    }
}

pub fn parse_mint(data: &[u8]) -> Result<MintCore, &'static str> {
    if data.len() < MINT_LEN {
        return Err("token data too short to be a valid mint");
    }
    Ok(MintCore {
        mint_authority: read_optional_key(&data[0..4], &data[4..36]),
        supply: u64::from_le_bytes(data[36..44].try_into().unwrap()),
        decimals: data[44],
        is_initialized: data[45] != 0,
        freeze_authority: read_optional_key(&data[46..50], &data[50..82]),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Extension {
    TransferFee,
    MintCloseAuthority,
    DefaultAccountState,
    NonTransferable,
    PermanentDelegate,
    TransferHook,
    MetadataPointer,
    TokenMetadata,
    Unknown(u16),
}

const EXT_LIST_START: usize = 166;

pub fn parse_extensions(data: &[u8]) -> Vec<Extension> {
    let mut out = Vec::new();
    if data.len() <= EXT_LIST_START {
        return out;
    }
    let mut i = EXT_LIST_START;
    while i + 4 <= data.len() {
        let ext_type = u16::from_le_bytes([data[i], data[i + 1]]);
        let ext_len = u16::from_le_bytes([data[i + 2], data[i + 3]]) as usize;
        let end = i + 4 + ext_len;
        if ext_type == 0 || end > data.len() {
            break;
        }
        out.push(match ext_type {
            1 => Extension::TransferFee,
            3 => Extension::MintCloseAuthority,
            6 => Extension::DefaultAccountState,
            9 => Extension::NonTransferable,
            12 => Extension::PermanentDelegate,
            14 => Extension::TransferHook,
            18 => Extension::MetadataPointer,
            19 => Extension::TokenMetadata,
            other => Extension::Unknown(other),
        });
        i = end;
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Concentration {
    pub top1_pct: f64,
    pub top10_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Risk {
    Green,
    Amber,
    Red,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub severity: Risk,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskReport {
    pub risk: Risk,
    pub signals: Vec<Signal>,
}

fn signal(severity: Risk, reason: &str) -> Signal {
    Signal { severity, reason: reason.to_string() }
}

pub fn score(
    mint: &MintCore,
    exts: &[Extension],
    conc: Option<&Concentration>,
    verified: bool,
) -> RiskReport {
    let mut signals = Vec::new();

    let has_mint = mint.mint_authority.is_some();
    let has_freeze = mint.freeze_authority.is_some();
    if verified && has_mint && has_freeze {
        signals.push(signal(Risk::Amber,
            "mint & freeze authority retained by the issuer (verified)"));
    } else {
        if has_mint {
            if verified {
                signals.push(signal(Risk::Amber,
                    "mint authority retained by the issuer (verified)"));
            } else {
                signals.push(signal(Risk::Red,
                    "unknown issuer can mint unlimited supply"));
            }
        }
        if has_freeze {
            if verified {
                signals.push(signal(Risk::Amber,
                    "freeze authority retained by the issuer (verified)"));
            } else {
                signals.push(signal(Risk::Red,
                    "unknown issuer can freeze wallets"));
            }
        }
    }

    for e in exts {
        let s = match e {
            Extension::PermanentDelegate => signal(Risk::Red,
                "permanent delegate can seize any wallet"),
            Extension::NonTransferable => signal(Risk::Red,
                "non-transferable, cannot be sent"),
            Extension::TransferHook => signal(Risk::Amber,
                "transfer hook runs on every transfer"),
            Extension::TransferFee => signal(Risk::Amber,
                "transfer fee on every transfer"),
            Extension::DefaultAccountState => signal(Risk::Amber,
                "new accounts may start frozen"),
            Extension::MintCloseAuthority => signal(Risk::Amber,
                "mint can be closed"),
            Extension::MetadataPointer | Extension::TokenMetadata => continue,
            Extension::Unknown(t) => signal(Risk::Amber,
                &format!("unknown extension (type {t})")),
        };
        signals.push(s);
    }

    if let Some(c) = conc {
        if c.top1_pct >= 50.0 {
            signals.push(signal(Risk::Amber, &format!("top holder owns {:.0}%", c.top1_pct)));
        } else if c.top10_pct >= 70.0 {
            signals.push(signal(Risk::Amber, &format!("top 10 hold {:.0}%", c.top10_pct)));
        }
    }

    let risk = if signals.iter().any(|s| s.severity == Risk::Red) {
        Risk::Red
    } else if signals.iter().any(|s| s.severity == Risk::Amber) {
        Risk::Amber
    } else {
        Risk::Green
    };

    RiskReport { risk, signals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_mint() -> Vec<u8> {
        let mut d = vec![0u8; 82];
        d[36..44].copy_from_slice(&1000u64.to_le_bytes());
        d[44] = 6;
        d[45] = 1;
        d[46..50].copy_from_slice(&1u32.to_le_bytes());
        for b in &mut d[50..82] {
            *b = 7;
        }
        d
    }

    #[test]
    fn reads_basic_facts() {
        let m = parse_mint(&fake_mint()).unwrap();
        assert!(m.mint_authority.is_none());
        assert_eq!(m.supply, 1000);
        assert_eq!(m.decimals, 6);
        assert!(m.is_initialized);
        assert_eq!(m.freeze_authority, Some([7u8; 32]));
    }

    #[test]
    fn rejects_too_short_data() {
        assert!(parse_mint(&[0u8; 10]).is_err());
    }

    #[test]
    fn finds_permanent_delegate_extension() {
        let mut d = vec![0u8; EXT_LIST_START];
        d.extend_from_slice(&12u16.to_le_bytes());
        d.extend_from_slice(&32u16.to_le_bytes());
        d.extend_from_slice(&[9u8; 32]);
        assert_eq!(parse_extensions(&d), vec![Extension::PermanentDelegate]);
    }

    #[test]
    fn legacy_mint_has_no_extensions() {
        assert!(parse_extensions(&[0u8; 82]).is_empty());
    }

    #[test]
    fn unknown_extension_type_is_kept_as_unknown() {
        let mut d = vec![0u8; EXT_LIST_START];
        d.extend_from_slice(&999u16.to_le_bytes());
        d.extend_from_slice(&4u16.to_le_bytes());
        d.extend_from_slice(&[1u8; 4]);
        assert_eq!(parse_extensions(&d), vec![Extension::Unknown(999)]);
    }

    #[test]
    fn length_overrun_stops_without_panic() {
        let mut d = vec![0u8; EXT_LIST_START];
        d.extend_from_slice(&12u16.to_le_bytes());
        d.extend_from_slice(&200u16.to_le_bytes());
        d.extend_from_slice(&[0u8; 4]);
        assert!(parse_extensions(&d).is_empty());
    }

    #[test]
    fn truncated_mid_header_stops_without_panic() {
        let mut d = vec![0u8; EXT_LIST_START];
        d.extend_from_slice(&[9u8, 0]);
        assert!(parse_extensions(&d).is_empty());
    }

    #[test]
    fn second_extension_truncated_keeps_only_the_valid_first() {
        let mut d = vec![0u8; EXT_LIST_START];
        d.extend_from_slice(&9u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&14u16.to_le_bytes());
        d.extend_from_slice(&50u16.to_le_bytes());
        assert_eq!(parse_extensions(&d), vec![Extension::NonTransferable]);
    }

    fn clean_mint() -> MintCore {
        MintCore {
            mint_authority: None,
            supply: 1000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: None,
        }
    }

    #[test]
    fn clean_token_is_green() {
        let r = score(&clean_mint(), &[], None, false);
        assert_eq!(r.risk, Risk::Green);
        assert!(r.signals.is_empty());
    }

    #[test]
    fn active_mint_authority_on_unknown_token_is_red() {
        let mut m = clean_mint();
        m.mint_authority = Some([1u8; 32]);
        assert_eq!(score(&m, &[], None, false).risk, Risk::Red);
    }

    #[test]
    fn active_authority_on_verified_issuer_is_amber_not_red() {
        let mut m = clean_mint();
        m.mint_authority = Some([1u8; 32]);
        m.freeze_authority = Some([1u8; 32]);
        let r = score(&m, &[], None, true);
        assert_eq!(r.risk, Risk::Amber);
        assert!(r.signals.iter().any(|s| s.reason.contains("retained by the issuer")));
    }

    #[test]
    fn verification_does_not_soften_dangerous_extensions() {
        assert_eq!(
            score(&clean_mint(), &[Extension::PermanentDelegate], None, true).risk,
            Risk::Red
        );
    }

    #[test]
    fn permanent_delegate_is_red() {
        assert_eq!(score(&clean_mint(), &[Extension::PermanentDelegate], None, false).risk, Risk::Red);
    }

    #[test]
    fn transfer_hook_only_is_amber() {
        assert_eq!(score(&clean_mint(), &[Extension::TransferHook], None, false).risk, Risk::Amber);
    }

    #[test]
    fn metadata_extensions_are_benign() {
        let exts = [Extension::MetadataPointer, Extension::TokenMetadata];
        assert_eq!(score(&clean_mint(), &exts, None, false).risk, Risk::Green);
    }

    #[test]
    fn high_top1_concentration_is_amber() {
        let c = Concentration { top1_pct: 60.0, top10_pct: 75.0 };
        assert_eq!(score(&clean_mint(), &[], Some(&c), false).risk, Risk::Amber);
    }

    #[test]
    fn low_concentration_stays_green() {
        let c = Concentration { top1_pct: 5.0, top10_pct: 20.0 };
        assert_eq!(score(&clean_mint(), &[], Some(&c), false).risk, Risk::Green);
    }
}
