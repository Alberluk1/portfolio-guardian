use portfolio_brief::portfolio::sanitize_symbol;
use portfolio_brief::token_risk::{parse_extensions, parse_mint};
use proptest::prelude::*;

proptest! {
    #[test]
    fn sanitize_output_is_always_safe(s in any::<String>()) {
        let out = sanitize_symbol(&s);
        prop_assert!(!out.is_empty());
        prop_assert!(out.chars().count() <= 33);
        for c in out.chars() {
            prop_assert!(!c.is_control());
            let cp = c as u32;
            let dangerous = matches!(cp,
                0x200B..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF);
            prop_assert!(!dangerous, "leaked U+{cp:04X}");
            prop_assert!(!matches!(c, '_' | '*' | '`' | '[' | ']' | '~' | '|'));
        }
    }

    #[test]
    fn parse_mint_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let _ = parse_mint(&bytes);
    }

    #[test]
    fn parse_extensions_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = parse_extensions(&bytes);
    }

    #[test]
    fn mint_with_random_extension_region_is_stable(tail in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut data = vec![0u8; 82];
        data[45] = 1;
        data.extend_from_slice(&tail);
        let mint = parse_mint(&data).expect("82-byte prefix parses");
        prop_assert!(mint.is_initialized);
        let _ = parse_extensions(&data);
    }
}
