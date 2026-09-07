//! The one composition of a prefixed result-set column name.
//!
//! PostgreSQL truncates identifiers to 63 bytes (`NAMEDATALEN - 1`) — an
//! alias past the bound is silently cut server-side, so a projection minting
//! `s0_{61 chars}` and a decode requesting all 64 bytes would never meet.
//! Every prefixed name is therefore composed here, by the writers and the
//! readers alike, so both sides of the contract spell a long name the same
//! bounded way.

/// PostgreSQL's identifier bound: `NAMEDATALEN - 1` bytes.
const IDENT_MAX_BYTES: usize = 63;

/// The result-set name of `col` under `pre`: the plain concatenation when it
/// fits PostgreSQL's 63-byte identifier bound, otherwise the longest
/// UTF-8-whole head that leaves room followed by the 64-bit FNV-1a hash of
/// the full name as 16 hex digits. The hash covers the whole name, so two
/// long names sharing a head still get distinct spellings.
// [spec:pgorm:sem:query.graph.writer+2]
pub(crate) fn result_column_name(pre: &str, col: &str) -> String {
    let full = format!("{pre}{col}");
    if full.len() <= IDENT_MAX_BYTES {
        return full;
    }
    let hash = fnv1a(full.as_bytes());
    let mut end = IDENT_MAX_BYTES - 16;
    while !full.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{hash:016x}", &full[..end])
}

/// 64-bit FNV-1a. The spelling of a bounded alias must be reproducible by
/// the decode of another build, so the hash has to be stable across
/// releases and platforms — which `DefaultHasher` does not guarantee.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:sem:query.graph.writer+2/test]    names within the bound pass
    // through untouched; names past it are bounded, deterministic, distinct
    // for shared heads, and never split a UTF-8 sequence
    #[test]
    fn bounded_names_stay_within_the_identifier_limit() {
        assert_eq!(result_column_name("s0_", "id"), "s0_id");
        assert_eq!(result_column_name("", "name"), "name");

        let long_a = "a".repeat(61);
        let bounded = result_column_name("s0_", &long_a);
        assert_eq!(bounded.len(), IDENT_MAX_BYTES);
        assert!(bounded.starts_with("s0_"));
        assert_eq!(bounded, result_column_name("s0_", &long_a));

        // Two names sharing a 47-byte head differ only past the head, so
        // only the hash tells them apart.
        let long_b = format!("{}b", "a".repeat(60));
        assert_ne!(bounded, result_column_name("s0_", &long_b));

        // A multibyte name: the head never splits a UTF-8 sequence.
        let wide = "å".repeat(40);
        let bounded = result_column_name("s0_", &wide);
        assert!(bounded.len() <= IDENT_MAX_BYTES);
        assert!(bounded.starts_with("s0_å"));
    }
}
