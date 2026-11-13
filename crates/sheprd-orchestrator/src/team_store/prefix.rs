//! Resolving a user-supplied hexadecimal prefix against one entity kind's
//! IDs: the one rule task, post, finding, and member references share.

use super::{Error, Result};

/// Shortest accepted hexadecimal prefix when referencing a team-scoped ID.
pub const MIN_ID_PREFIX_CHARS: usize = 4;

/// Candidates shown in an [`Error::AmbiguousId`] message before eliding.
const AMBIGUOUS_CANDIDATES_SHOWN: usize = 8;

/// Lowercased token with UUID dashes stripped — the key hex-prefix matching
/// runs over, so dashed and undashed prefixes are equivalent.
fn id_prefix_key(token: &str) -> String {
    token
        .to_ascii_lowercase()
        .chars()
        .filter(|c| *c != '-')
        .collect()
}

fn candidate_list(candidates: &[&String]) -> String {
    let shown = candidates
        .iter()
        .take(AMBIGUOUS_CANDIDATES_SHOWN)
        .map(|id| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if candidates.len() > AMBIGUOUS_CANDIDATES_SHOWN {
        format!(
            "{shown}, … +{} more",
            candidates.len() - AMBIGUOUS_CANDIDATES_SHOWN
        )
    } else {
        shown
    }
}

/// Resolves a user-supplied reference against the IDs of one entity kind.
///
/// An exact ID passes through unchanged, as does anything that is not a
/// hexadecimal token of at least [`MIN_ID_PREFIX_CHARS`] characters after
/// dashes are stripped (display names and aliases are resolved by their own
/// rules). A hex token that prefix-matches exactly one candidate resolves to
/// it, case-insensitively, whether or not it carries UUID dashes; several
/// matches are refused with the candidates listed, and no match passes through
/// unchanged so the caller reports its usual not-found error.
pub(super) fn resolve_id_prefix(
    kind: &'static str,
    candidates: &[String],
    token: &str,
) -> Result<String> {
    if candidates.iter().any(|id| id == token) {
        return Ok(token.to_owned());
    }
    let needle = id_prefix_key(token);
    if needle.len() < MIN_ID_PREFIX_CHARS || !needle.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(token.to_owned());
    }
    let matches: Vec<&String> = candidates
        .iter()
        .filter(|id| id_prefix_key(id).starts_with(&needle))
        .collect();
    match matches.as_slice() {
        [] => Ok(token.to_owned()),
        [single] => Ok((*single).clone()),
        many => Err(Error::AmbiguousId {
            kind,
            token: token.to_owned(),
            candidates: candidate_list(many),
        }),
    }
}
