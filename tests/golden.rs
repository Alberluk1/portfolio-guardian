use base64::{engine::general_purpose::STANDARD, Engine};
use portfolio_brief::token_risk::{parse_extensions, parse_mint, score, Extension, Risk};

const PYUSD_B64: &str = include_str!("fixtures/pyusd_mint.b64");
const USDC_B64: &str = include_str!("fixtures/usdc_mint.b64");

fn pyusd_bytes() -> Vec<u8> {
    STANDARD.decode(PYUSD_B64.trim()).expect("valid base64 fixture")
}

fn usdc_bytes() -> Vec<u8> {
    STANDARD.decode(USDC_B64.trim()).expect("valid base64 fixture")
}

#[test]
fn usdc_classic_mint_parses_with_no_extensions() {
    let data = usdc_bytes();
    assert_eq!(data.len(), 82);
    let mint = parse_mint(&data).unwrap();
    assert!(mint.is_initialized);
    assert!(mint.mint_authority.is_some());
    assert!(mint.freeze_authority.is_some());
    assert!(parse_extensions(&data).is_empty());
}

#[test]
fn usdc_is_amber_when_verified_red_when_not() {
    let mint = parse_mint(&usdc_bytes()).unwrap();
    assert_eq!(score(&mint, &[], None, true).risk, Risk::Amber);
    assert_eq!(score(&mint, &[], None, false).risk, Risk::Red);
}

#[test]
fn pyusd_base_mint_parses() {
    let mint = parse_mint(&pyusd_bytes()).expect("parse base mint");
    assert!(mint.is_initialized);
}

#[test]
fn pyusd_real_extensions_detected() {
    let exts = parse_extensions(&pyusd_bytes());
    assert!(exts.contains(&Extension::PermanentDelegate));
    assert!(exts.contains(&Extension::TransferHook));
    assert!(exts.contains(&Extension::TransferFee));
    assert!(exts.contains(&Extension::MintCloseAuthority));
}

#[test]
fn pyusd_scores_red_from_real_data() {
    let data = pyusd_bytes();
    let mint = parse_mint(&data).unwrap();
    let exts = parse_extensions(&data);
    let report = score(&mint, &exts, None, true);
    assert_eq!(report.risk, Risk::Red);
    assert!(!report.signals.is_empty());
}
