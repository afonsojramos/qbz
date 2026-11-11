//! Artist identity matching — the #768 contract.
//!
//! Two directions, deliberately NOT one name-keyed helper:
//!
//! 1. **Source artist** (the Qobuz artist whose page is open) → MusicBrainz
//!    MBID. Evidence: the exact quoted-name search window plus the ISRCs of
//!    the page's own Popular Tracks, resolved to MusicBrainz recording
//!    artist credits.
//! 2. **Scene candidate** (a MusicBrainz MBID discovered by tag/area) → Qobuz
//!    artist id. Evidence: exact-name Qobuz candidates plus, only when there
//!    are several, the ISRCs of each candidate's own top tracks.
//!
//! Everything in this module is either a pure decision function or an
//! orchestration over injected [`IdentityFetcher`] / [`IdentityStore`] /
//! [`SceneCatalog`] implementations, so every rule is unit-testable with no
//! network and no SQLite. The MusicBrainz client implements the fetcher, the
//! SQLite cache implements the store, and `QbzCore` implements the catalog.
//!
//! Invariants (from the guardrails, `qbz-nix-docs/research/2026-09-11-768-…`):
//! - score, result order, `albums_count`, top-track order and string
//!   similarity are NEVER identity; they only decide what to inspect;
//! - an unrelated first result is structurally impossible: the only way out
//!   of a name search is [`select_exact_name`], which returns a candidate
//!   only when EXACTLY ONE result has the requested name and score >= 90;
//! - every artist credit of every recording that really carries the ISRC is
//!   inspected; one ISRC is one vote no matter how many rows repeat it;
//! - ties, conflicts and failures fail closed (no identity, no cache write);
//! - durable identity rows are keyed by the stable identifier on the known
//!   side (Qobuz artist id → MBID; MBID + catalog scope → Qobuz id), carry
//!   their evidence kind and matcher version, and expire at read time.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use super::models::{
    ArtistResult, ArtistType, MatchConfidence, RecordingSearchResponse, ResolvedArtist,
};

/// Bumped whenever the acceptance/voting semantics change, so rows written by
/// an older matcher can never satisfy a newer lookup.
pub const MATCHER_VERSION: u32 = 2;
/// Minimum MusicBrainz search score for an exact-name candidate.
pub const EXACT_NAME_MIN_SCORE: i32 = 90;
/// Size of the quoted-name search window.
pub const NAME_SEARCH_LIMIT: u32 = 10;
/// Distinct Popular-Track ISRCs inspected for one source artist.
pub const MAX_SOURCE_ISRCS: usize = 3;
/// Distinct top-track ISRCs inspected per ambiguous scene candidate.
pub const MAX_SCENE_ISRCS_PER_CANDIDATE: usize = 2;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ============ Query construction ============

/// Escape Lucene special characters in a single term.
pub fn lucene_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(':', "\\:")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('^', "\\^")
        .replace('~', "\\~")
        .replace('*', "\\*")
        .replace('?', "\\?")
        .replace('!', "\\!")
        .replace('+', "\\+")
        .replace('-', "\\-")
        .replace('&', "\\&")
        .replace('|', "\\|")
}

/// The complete fielded expression for an exact artist-name search:
/// `artist:"<escaped name>"`. The name is escaped, then quoted, then the
/// CALLER URL-encodes the whole expression exactly once
/// ([`artist_search_url`]). Unquoted, a multi-word name lets its second word
/// leak into the global query — `artist:Swim Deep` ranks Deep Purple first
/// (#768).
pub fn artist_query_expression(name: &str) -> String {
    format!("artist:\"{}\"", lucene_escape(name.trim()))
}

/// Full search URL for the quoted-name query against `base`.
pub fn artist_search_url(base: &str, name: &str, limit: u32) -> String {
    format!(
        "{}/artist?query={}&limit={}&fmt=json",
        base,
        urlencoding::encode(&artist_query_expression(name)),
        limit
    )
}

/// Key of the cached search window for `name` — the exact expression plus
/// the window size and matcher version, so a coarser normalisation can never
/// share a slot between two different queries.
pub fn artist_query_key(name: &str) -> String {
    format!(
        "v{}|{}|{}",
        MATCHER_VERSION,
        NAME_SEARCH_LIMIT,
        artist_query_expression(name)
    )
}

/// ISRC normalised once at the boundary: trimmed, hyphens/spaces dropped,
/// upper-cased. `None` when nothing usable remains.
pub fn normalize_isrc(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

// ============ Name candidates ============

/// One row of a MusicBrainz artist search window, in the shape that is
/// cached. Deliberately a value type separate from the wire `ArtistResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistCandidate {
    pub mbid: String,
    pub name: String,
    pub score: Option<i32>,
    #[serde(default)]
    pub sort_name: Option<String>,
    #[serde(default)]
    pub artist_type: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub disambiguation: Option<String>,
}

impl From<&ArtistResult> for ArtistCandidate {
    fn from(a: &ArtistResult) -> Self {
        Self {
            mbid: a.id.clone(),
            name: a.name.clone(),
            score: a.score,
            sort_name: a.sort_name.clone(),
            artist_type: a.artist_type.clone(),
            country: a.country.clone(),
            disambiguation: a.disambiguation.clone(),
        }
    }
}

impl ArtistCandidate {
    /// The legacy `ResolvedArtist` shape. `confidence` is derived from the
    /// search score and is METADATA of a name match; it never means
    /// "ISRC verified".
    pub fn to_resolved(&self) -> ResolvedArtist {
        ResolvedArtist {
            mbid: self.mbid.clone(),
            name: self.name.clone(),
            sort_name: self.sort_name.clone(),
            artist_type: ArtistType::from(self.artist_type.as_deref()),
            country: self.country.clone(),
            disambiguation: self.disambiguation.clone(),
            confidence: MatchConfidence::from_score(self.score),
        }
    }
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim()) || a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Every candidate in the window whose trimmed, case-insensitive name equals
/// the requested one and whose score is at least [`EXACT_NAME_MIN_SCORE`].
/// No punctuation stripping, no fuzzy distance, no aliases.
pub fn exact_name_candidates<'a>(name: &str, window: &'a [ArtistCandidate]) -> Vec<&'a ArtistCandidate> {
    window
        .iter()
        .filter(|c| same_name(&c.name, name) && c.score.unwrap_or(0) >= EXACT_NAME_MIN_SCORE)
        .collect()
}

/// Containment rule: exactly one acceptable exact-name candidate, or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExactNameSelection<'a> {
    Unique(&'a ArtistCandidate),
    /// Several acceptable same-name candidates — a homonym the name cannot
    /// split. Carries the count for logs.
    Ambiguous(usize),
    None,
}

pub fn select_exact_name<'a>(name: &str, window: &'a [ArtistCandidate]) -> ExactNameSelection<'a> {
    let exact = exact_name_candidates(name, window);
    match exact.len() {
        0 => ExactNameSelection::None,
        1 => ExactNameSelection::Unique(exact[0]),
        n => ExactNameSelection::Ambiguous(n),
    }
}

// ============ ISRC evidence ============

/// One MusicBrainz recording that really carries the queried ISRC, with
/// EVERY artist credit on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingCredits {
    pub recording_mbid: String,
    pub credit_mbids: Vec<String>,
}

/// The complete matching-recording set for one normalised ISRC. This — not a
/// first-result pick — is what gets cached and voted on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsrcCredits {
    pub isrc: String,
    pub recordings: Vec<RecordingCredits>,
}

impl IsrcCredits {
    /// Keep only the recordings whose own `isrcs` list contains the queried
    /// ISRC (case-insensitively). A search hit that merely SCORES against
    /// the ISRC contributes nothing.
    pub fn from_search(isrc: &str, response: &RecordingSearchResponse) -> Self {
        let wanted = normalize_isrc(isrc).unwrap_or_default();
        let recordings = response
            .recordings
            .iter()
            .filter(|r| {
                r.isrcs
                    .as_ref()
                    .map(|list| {
                        list.iter()
                            .any(|i| normalize_isrc(i).as_deref() == Some(wanted.as_str()))
                    })
                    .unwrap_or(false)
            })
            .map(|r| RecordingCredits {
                recording_mbid: r.id.clone(),
                credit_mbids: r
                    .artist_credit
                    .as_ref()
                    .map(|ac| ac.iter().map(|c| c.artist.id.clone()).collect())
                    .unwrap_or_default(),
            })
            .collect();
        Self {
            isrc: wanted,
            recordings,
        }
    }

    /// Union of every credit MBID across every matching recording.
    pub fn credited(&self) -> BTreeSet<&str> {
        self.recordings
            .iter()
            .flat_map(|r| r.credit_mbids.iter().map(String::as_str))
            .collect()
    }
}

/// The page's own Popular-Track ISRCs, in server order, deduplicated after
/// normalisation and capped. Tracks credited to another artist id (guest
/// appearances) are excluded — they cannot establish the page identity.
pub fn pick_source_isrcs<'a, I>(source_artist_id: u64, tracks: I, cap: usize) -> Vec<String>
where
    I: IntoIterator<Item = (Option<&'a str>, Option<u64>)>,
{
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (isrc, artist_id) in tracks {
        if out.len() >= cap {
            break;
        }
        if artist_id != Some(source_artist_id) {
            continue;
        }
        let Some(norm) = isrc.and_then(normalize_isrc) else {
            continue;
        };
        if seen.insert(norm.clone()) {
            out.push(norm);
        }
    }
    out
}

// ============ Decisions ============

/// Provenance of a durable identity row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// One uniquely intersecting ISRC, or a unique winner over >= 2 ISRCs.
    VerifiedIsrc,
    /// Exactly one acceptable result in the quoted-name window; provisional.
    UniqueExactName,
}

impl EvidenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedIsrc => "verified_isrc",
            Self::UniqueExactName => "unique_exact_name",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "verified_isrc" => Some(Self::VerifiedIsrc),
            "unique_exact_name" => Some(Self::UniqueExactName),
            _ => None,
        }
    }
}

/// Evidence state of a source-artist resolution. Only `VerifiedIsrc` may be
/// written to the durable identifier-keyed cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceIdentity {
    VerifiedIsrc { mbid: String, isrc_votes: usize },
    UniqueExactName { mbid: String, score: Option<i32> },
    Ambiguous,
    NotFound,
    Unavailable,
}

impl SourceIdentity {
    /// The MBID a caller may DISPLAY (verified or provisional). `None` for
    /// every other state.
    pub fn displayable_mbid(&self) -> Option<&str> {
        match self {
            Self::VerifiedIsrc { mbid, .. } | Self::UniqueExactName { mbid, .. } => Some(mbid),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::VerifiedIsrc { .. } => "verified_isrc",
            Self::UniqueExactName { .. } => "unique_exact_name",
            Self::Ambiguous => "ambiguous",
            Self::NotFound => "not_found",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Vote among the exact-name candidates with the ISRC evidence.
///
/// - one ISRC votes for a candidate only when its credits intersect the
///   candidate set in EXACTLY ONE MBID (a collaboration credited to two
///   homonyms is not a vote);
/// - votes for two different MBIDs are a conflict → `Ambiguous`;
/// - one MBID with >= 1 unique vote wins, unless another candidate appears on
///   MORE distinct ISRCs than it (contradictory evidence → `Ambiguous`);
/// - with no unique vote at all, a candidate that appears on >= 2 distinct
///   ISRCs, strictly more than any other, wins;
/// - with no ISRC touching the candidates, the name rule applies: one
///   candidate is `UniqueExactName`, several are `Ambiguous`.
pub fn decide_source_identity(
    candidates: &[&ArtistCandidate],
    evidence: &[IsrcCredits],
) -> SourceIdentity {
    if candidates.is_empty() {
        return SourceIdentity::NotFound;
    }
    let set: BTreeSet<&str> = candidates.iter().map(|c| c.mbid.as_str()).collect();

    let mut unique_votes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    let mut seen_isrcs: BTreeSet<&str> = BTreeSet::new();
    for ev in evidence {
        // One ISRC, one vote — a repeated evidence row for the same ISRC
        // counts once.
        if !seen_isrcs.insert(ev.isrc.as_str()) {
            continue;
        }
        let credited = ev.credited();
        let intersection: Vec<&str> = set
            .iter()
            .copied()
            .filter(|m| credited.contains(m))
            .collect();
        for m in &intersection {
            *tally.entry(m).or_insert(0) += 1;
        }
        if intersection.len() == 1 {
            *unique_votes.entry(intersection[0]).or_insert(0) += 1;
        }
    }

    match unique_votes.len() {
        0 => {}
        1 => {
            let (winner, votes) = unique_votes.iter().next().map(|(m, v)| (*m, *v)).unwrap();
            let winner_tally = tally.get(winner).copied().unwrap_or(0);
            let contradicted = tally
                .iter()
                .any(|(m, count)| *m != winner && *count > winner_tally);
            if contradicted {
                return SourceIdentity::Ambiguous;
            }
            return SourceIdentity::VerifiedIsrc {
                mbid: winner.to_string(),
                isrc_votes: votes,
            };
        }
        _ => return SourceIdentity::Ambiguous,
    }

    if !tally.is_empty() {
        let mut ranked: Vec<(&str, usize)> = tally.iter().map(|(m, c)| (*m, *c)).collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let (top, top_count) = ranked[0];
        let second = ranked.get(1).map(|r| r.1).unwrap_or(0);
        if top_count >= 2 && top_count > second {
            return SourceIdentity::VerifiedIsrc {
                mbid: top.to_string(),
                isrc_votes: top_count,
            };
        }
        return SourceIdentity::Ambiguous;
    }

    if candidates.len() == 1 {
        SourceIdentity::UniqueExactName {
            mbid: candidates[0].mbid.clone(),
            score: candidates[0].score,
        }
    } else {
        SourceIdentity::Ambiguous
    }
}

/// A Qobuz artist that carries the scene candidate's exact name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneQobuzCandidate {
    pub qobuz_id: u64,
    pub name: String,
    pub image: Option<String>,
    pub albums_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneDecision {
    Verified { qobuz_id: u64, isrc_votes: usize },
    Unresolved,
}

/// For a known MBID, pick the Qobuz duplicate whose top-track ISRCs are
/// credited to that MBID. Several verified duplicates fall back to
/// `albums_count` as an OPERATIONAL tie-break (they are the same artist);
/// a tie there, or no verified duplicate at all, is `Unresolved`.
pub fn decide_scene_candidate(
    expected_mbid: &str,
    evidence: &[(u64, Option<u32>, Vec<IsrcCredits>)],
) -> SceneDecision {
    let mut verified: Vec<(u64, Option<u32>, usize)> = Vec::new();
    for (qobuz_id, albums_count, credits) in evidence {
        let mut voting_isrcs: BTreeSet<&str> = BTreeSet::new();
        for ev in credits {
            if ev.credited().contains(expected_mbid) {
                voting_isrcs.insert(ev.isrc.as_str());
            }
        }
        if !voting_isrcs.is_empty() {
            verified.push((*qobuz_id, *albums_count, voting_isrcs.len()));
        }
    }
    match verified.len() {
        0 => SceneDecision::Unresolved,
        1 => SceneDecision::Verified {
            qobuz_id: verified[0].0,
            isrc_votes: verified[0].2,
        },
        _ => {
            verified.sort_by(|a, b| b.1.unwrap_or(0).cmp(&a.1.unwrap_or(0)));
            if verified[0].1.unwrap_or(0) > verified[1].1.unwrap_or(0) {
                SceneDecision::Verified {
                    qobuz_id: verified[0].0,
                    isrc_votes: verified[0].2,
                }
            } else {
                SceneDecision::Unresolved
            }
        }
    }
}

// ============ Injected boundaries ============

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Disabled integration, rate limit after retries, transport or server
    /// failure. Never negative-cached.
    Unavailable(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(s) => write!(f, "unavailable: {s}"),
        }
    }
}

/// MusicBrainz reads. The core's shared client implements this; tests use
/// fixtures and count calls.
pub trait IdentityFetcher: Sync {
    /// The quoted-name search window for `name` (already limited).
    fn artist_candidates<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ArtistCandidate>, FetchError>>;
    /// Every recording carrying `isrc` (normalised) with all its credits.
    fn isrc_credits<'a>(&'a self, isrc: &'a str) -> BoxFuture<'a, Result<IsrcCredits, FetchError>>;
}

/// Durable source-identity row: Qobuz artist id → MBID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentityRow {
    pub qobuz_artist_id: u64,
    pub mbid: String,
    pub evidence_kind: EvidenceKind,
    pub evidence_count: u32,
    pub source_name: String,
}

/// Durable scene-identity row: MBID + catalog scope → Qobuz artist id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIdentityRow {
    pub mbid: String,
    pub scope: String,
    pub qobuz_id: u64,
    pub evidence_kind: EvidenceKind,
    pub evidence_count: u32,
    pub qobuz_name: String,
    pub image: Option<String>,
    pub albums_count: Option<u32>,
}

/// Cache boundary. Every getter applies TTL + matcher version itself and
/// treats a malformed row as a miss; failures are logged by the
/// implementation and surface as `None`.
pub trait IdentityStore: Sync {
    fn artist_query(&self, key: &str) -> Option<Vec<ArtistCandidate>>;
    fn put_artist_query(&self, key: &str, window: &[ArtistCandidate]);
    fn isrc_credits(&self, isrc: &str) -> Option<IsrcCredits>;
    fn put_isrc_credits(&self, credits: &IsrcCredits);
    /// Only `VerifiedIsrc` rows are ever returned for a source lookup.
    fn source_identity(&self, qobuz_artist_id: u64) -> Option<SourceIdentityRow>;
    fn put_source_identity(&self, row: &SourceIdentityRow);
    fn scene_identity(&self, mbid: &str, scope: &str) -> Option<SceneIdentityRow>;
    fn put_scene_identity(&self, row: &SceneIdentityRow);
}

/// Qobuz reads the scene direction needs. `QbzCore` implements this.
pub trait SceneCatalog: Sync {
    /// Exact-name Qobuz artists for `name`, from the bounded search.
    fn exact_name_candidates<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Vec<SceneQobuzCandidate>, FetchError>>;
    /// Normalised, deduplicated top-track ISRCs credited to `qobuz_id`
    /// itself, capped by the caller's bound.
    fn candidate_isrcs<'a>(
        &'a self,
        qobuz_id: u64,
        cap: usize,
    ) -> BoxFuture<'a, Result<Vec<String>, FetchError>>;
}

// ============ Orchestration ============

/// Cache-first search window for `name`; the acceptance rule runs again on
/// every hit (a cached window is evidence, not an identity).
async fn candidate_window(
    store: Option<&dyn IdentityStore>,
    fetcher: &dyn IdentityFetcher,
    name: &str,
    requests: &mut usize,
) -> Result<Vec<ArtistCandidate>, FetchError> {
    let key = artist_query_key(name);
    if let Some(window) = store.and_then(|s| s.artist_query(&key)) {
        return Ok(window);
    }
    *requests += 1;
    let window = fetcher.artist_candidates(name).await?;
    if let Some(s) = store {
        s.put_artist_query(&key, &window);
    }
    Ok(window)
}

async fn credits_for(
    store: Option<&dyn IdentityStore>,
    fetcher: &dyn IdentityFetcher,
    isrc: &str,
    requests: &mut usize,
) -> Result<IsrcCredits, FetchError> {
    if let Some(hit) = store.and_then(|s| s.isrc_credits(isrc)) {
        return Ok(hit);
    }
    *requests += 1;
    let credits = fetcher.isrc_credits(isrc).await?;
    if let Some(s) = store {
        s.put_isrc_credits(&credits);
    }
    Ok(credits)
}

/// The generic name-only resolver (playlist suggestions, musician lookups,
/// local artist images): containment rule, nothing more. Never writes a
/// durable identity — a name alone is not one.
pub async fn resolve_artist_by_name(
    store: Option<&dyn IdentityStore>,
    fetcher: &dyn IdentityFetcher,
    name: &str,
) -> Result<Option<ResolvedArtist>, FetchError> {
    let mut requests = 0;
    let window = candidate_window(store, fetcher, name, &mut requests).await?;
    Ok(match select_exact_name(name, &window) {
        ExactNameSelection::Unique(c) => Some(c.to_resolved()),
        ExactNameSelection::Ambiguous(n) => {
            log::info!("[mb-identity] name {name:?}: {n} exact-name candidates, unresolved");
            None
        }
        ExactNameSelection::None => None,
    })
}

/// Outcome of a source-artist resolution, with the numbers a log line needs.
#[derive(Debug, Clone)]
pub struct SourceResolution {
    pub identity: SourceIdentity,
    /// The candidate behind a displayable identity (legacy shape for the
    /// Artist Page), `None` otherwise.
    pub resolved: Option<ResolvedArtist>,
    /// MusicBrainz requests actually issued.
    pub requests: usize,
    pub cache: &'static str,
}

/// Direction 1: Qobuz artist id (+ name + own-track ISRCs) → MBID.
pub async fn resolve_source_artist(
    store: Option<&dyn IdentityStore>,
    fetcher: &dyn IdentityFetcher,
    qobuz_artist_id: u64,
    name: &str,
    isrcs: &[String],
) -> SourceResolution {
    // Verified identifier-keyed hit: zero requests.
    if let Some(row) = store.and_then(|s| s.source_identity(qobuz_artist_id)) {
        if row.evidence_kind == EvidenceKind::VerifiedIsrc && !row.mbid.is_empty() {
            return SourceResolution {
                identity: SourceIdentity::VerifiedIsrc {
                    mbid: row.mbid.clone(),
                    isrc_votes: row.evidence_count as usize,
                },
                resolved: Some(ResolvedArtist {
                    mbid: row.mbid,
                    name: row.source_name,
                    sort_name: None,
                    artist_type: ArtistType::Other,
                    country: None,
                    disambiguation: None,
                    confidence: MatchConfidence::Exact,
                }),
                requests: 0,
                cache: "hit",
            };
        }
    }

    let mut requests = 0;
    let window = match candidate_window(store, fetcher, name, &mut requests).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("[mb-identity] source {qobuz_artist_id} {name:?}: search failed: {e}");
            return SourceResolution {
                identity: SourceIdentity::Unavailable,
                resolved: None,
                requests,
                cache: "miss",
            };
        }
    };
    let exact = exact_name_candidates(name, &window);
    if exact.is_empty() {
        return SourceResolution {
            identity: SourceIdentity::NotFound,
            resolved: None,
            requests,
            cache: "miss",
        };
    }

    // Distinct ISRCs only, bounded, sequential through the shared client.
    let mut distinct: Vec<String> = Vec::new();
    for raw in isrcs {
        if distinct.len() >= MAX_SOURCE_ISRCS {
            break;
        }
        if let Some(norm) = normalize_isrc(raw) {
            if !distinct.contains(&norm) {
                distinct.push(norm);
            }
        }
    }
    let mut evidence: Vec<IsrcCredits> = Vec::new();
    let mut failures = 0usize;
    for isrc in &distinct {
        match credits_for(store, fetcher, isrc, &mut requests).await {
            Ok(c) => evidence.push(c),
            Err(e) => {
                failures += 1;
                log::warn!("[mb-identity] source {qobuz_artist_id}: isrc {isrc} failed: {e}");
            }
        }
    }

    let identity = decide_source_identity(&exact, &evidence);
    let identity = match identity {
        // Evidence was requested but none arrived and the name alone cannot
        // decide: that is a transient failure, not a verdict.
        SourceIdentity::Ambiguous if failures > 0 && evidence.is_empty() => {
            SourceIdentity::Unavailable
        }
        other => other,
    };

    let resolved = identity.displayable_mbid().and_then(|mbid| {
        exact
            .iter()
            .find(|c| c.mbid == mbid)
            .map(|c| c.to_resolved())
    });

    if let (SourceIdentity::VerifiedIsrc { mbid, isrc_votes }, Some(s)) = (&identity, store) {
        s.put_source_identity(&SourceIdentityRow {
            qobuz_artist_id,
            mbid: mbid.clone(),
            evidence_kind: EvidenceKind::VerifiedIsrc,
            evidence_count: *isrc_votes as u32,
            source_name: name.to_string(),
        });
    }

    log::info!(
        "[mb-identity] source qobuz={qobuz_artist_id} name={name:?}: {} (exact-name candidates {}, distinct isrcs {}, evidence {}, failures {}, mb requests {})",
        identity.label(),
        exact.len(),
        distinct.len(),
        evidence.len(),
        failures,
        requests
    );

    SourceResolution {
        identity,
        resolved,
        requests,
        cache: "miss",
    }
}

/// A scene candidate's Qobuz projection, with its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMatch {
    pub qobuz_id: u64,
    pub name: String,
    pub image: Option<String>,
    pub albums_count: Option<u32>,
    pub kind: EvidenceKind,
    /// MusicBrainz requests actually issued.
    pub requests: usize,
}

/// Direction 2: MBID (+ name + catalog scope) → Qobuz artist id.
///
/// `scope` is the authenticated catalog territory; `None` means the row can
/// be displayed but nothing is persisted (an invented constant would poison
/// the identifier-keyed cache across territories).
pub async fn resolve_scene_candidate(
    store: Option<&dyn IdentityStore>,
    fetcher: &dyn IdentityFetcher,
    catalog: &dyn SceneCatalog,
    expected_mbid: &str,
    name: &str,
    scope: Option<&str>,
    blacklist: &(dyn Fn(u64) -> bool + Sync),
) -> Result<Option<SceneMatch>, FetchError> {
    if let (Some(s), Some(scope)) = (store, scope) {
        if let Some(row) = s.scene_identity(expected_mbid, scope) {
            // A since-blacklisted hit is a MISS, not "no match": the next
            // same-name candidate takes the slot on the fresh path below.
            if row.qobuz_id > 0 && !blacklist(row.qobuz_id) {
                return Ok(Some(SceneMatch {
                    qobuz_id: row.qobuz_id,
                    name: row.qobuz_name,
                    image: row.image,
                    albums_count: row.albums_count,
                    kind: row.evidence_kind,
                    requests: 0,
                }));
            }
        }
    }

    let candidates: Vec<SceneQobuzCandidate> = catalog
        .exact_name_candidates(name)
        .await?
        .into_iter()
        .filter(|c| !blacklist(c.qobuz_id))
        .collect();

    let persist = |kind: EvidenceKind, count: usize, c: &SceneQobuzCandidate| {
        if let (Some(s), Some(scope)) = (store, scope) {
            s.put_scene_identity(&SceneIdentityRow {
                mbid: expected_mbid.to_string(),
                scope: scope.to_string(),
                qobuz_id: c.qobuz_id,
                evidence_kind: kind,
                evidence_count: count as u32,
                qobuz_name: c.name.clone(),
                image: c.image.clone(),
                albums_count: c.albums_count,
            });
        }
    };

    match candidates.len() {
        0 => Ok(None),
        1 => {
            // Unique exact name: the cheap path, provisional, never labelled
            // verified. No ISRC fan-out for a unique name.
            let c = &candidates[0];
            persist(EvidenceKind::UniqueExactName, 0, c);
            Ok(Some(SceneMatch {
                qobuz_id: c.qobuz_id,
                name: c.name.clone(),
                image: c.image.clone(),
                albums_count: c.albums_count,
                kind: EvidenceKind::UniqueExactName,
                requests: 0,
            }))
        }
        n => {
            let mut requests = 0usize;
            let mut evidence: Vec<(u64, Option<u32>, Vec<IsrcCredits>)> = Vec::new();
            for c in &candidates {
                let isrcs = match catalog
                    .candidate_isrcs(c.qobuz_id, MAX_SCENE_ISRCS_PER_CANDIDATE)
                    .await
                {
                    Ok(list) => list,
                    Err(e) => {
                        log::warn!(
                            "[mb-identity] scene {expected_mbid}: qobuz {} isrcs failed: {e}",
                            c.qobuz_id
                        );
                        Vec::new()
                    }
                };
                let mut credits = Vec::new();
                for isrc in isrcs.iter().take(MAX_SCENE_ISRCS_PER_CANDIDATE) {
                    match credits_for(store, fetcher, isrc, &mut requests).await {
                        Ok(cr) => credits.push(cr),
                        Err(e) => log::warn!(
                            "[mb-identity] scene {expected_mbid}: isrc {isrc} failed: {e}"
                        ),
                    }
                }
                evidence.push((c.qobuz_id, c.albums_count, credits));
            }
            let decision = decide_scene_candidate(expected_mbid, &evidence);
            log::info!(
                "[mb-identity] scene mbid={expected_mbid} name={name:?}: {n} exact-name qobuz candidates, decision {:?}, mb requests {requests}",
                decision
            );
            match decision {
                SceneDecision::Verified {
                    qobuz_id,
                    isrc_votes,
                } => {
                    let c = candidates
                        .iter()
                        .find(|c| c.qobuz_id == qobuz_id)
                        .expect("winner comes from the candidate list");
                    persist(EvidenceKind::VerifiedIsrc, isrc_votes, c);
                    Ok(Some(SceneMatch {
                        qobuz_id,
                        name: c.name.clone(),
                        image: c.image.clone(),
                        albums_count: c.albums_count,
                        kind: EvidenceKind::VerifiedIsrc,
                        requests,
                    }))
                }
                // Never the most prolific unverified homonym.
                SceneDecision::Unresolved => Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn cand(mbid: &str, name: &str, score: i32) -> ArtistCandidate {
        ArtistCandidate {
            mbid: mbid.into(),
            name: name.into(),
            score: Some(score),
            sort_name: None,
            artist_type: None,
            country: None,
            disambiguation: None,
        }
    }

    fn credits(isrc: &str, recordings: &[&[&str]]) -> IsrcCredits {
        IsrcCredits {
            isrc: normalize_isrc(isrc).unwrap(),
            recordings: recordings
                .iter()
                .enumerate()
                .map(|(i, c)| RecordingCredits {
                    recording_mbid: format!("rec-{i}"),
                    credit_mbids: c.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
        }
    }

    const SWIM_DEEP: &str = "85e67b0f-afd2-46c2-88b2-c3ba9b0883f2";
    const DEEP_PURPLE: &str = "79491354-3d83-40e3-9d8e-7592d58d790a";

    /// The #768 window, as observed live on 2026-09-11 for the UNQUOTED
    /// query: Deep Purple first at 100, Swim Deep second at 88.
    fn swim_deep_window() -> Vec<ArtistCandidate> {
        vec![
            cand(DEEP_PURPLE, "Deep Purple", 100),
            cand(SWIM_DEEP, "Swim Deep", 88),
            cand("x-deep-blue", "Deep Blue Something", 70),
        ]
    }

    // ----- query construction -------------------------------------------

    #[test]
    fn query_expression_quotes_the_whole_name() {
        assert_eq!(artist_query_expression("Swim Deep"), "artist:\"Swim Deep\"");
        assert_eq!(artist_query_expression("  Swim Deep  "), "artist:\"Swim Deep\"");
    }

    #[test]
    fn search_url_encodes_the_expression_exactly_once() {
        let url = artist_search_url("https://mb/ws/2", "Swim Deep", 10);
        assert_eq!(
            url,
            "https://mb/ws/2/artist?query=artist%3A%22Swim%20Deep%22&limit=10&fmt=json"
        );
    }

    #[test]
    fn lucene_specials_are_escaped_before_url_encoding_not_double_encoded() {
        let expr = artist_query_expression("AC/DC: \"Live\" (50%)");
        assert_eq!(expr, "artist:\"AC/DC\\: \\\"Live\\\" \\(50%\\)\"");
        let url = artist_search_url("b", "AC/DC: \"Live\" (50%)", 10);
        // Backslash -> %5C, quote -> %22, percent -> %25, each once.
        assert!(url.contains("%5C%3A"), "{url}");
        assert!(url.contains("%5C%22Live%5C%22"), "{url}");
        assert!(url.contains("50%25%5C%29"), "{url}");
        assert!(!url.contains("%2522"), "double-encoded percent: {url}");
        assert!(!url.contains("%255C"), "double-encoded backslash: {url}");
    }

    #[test]
    fn query_key_carries_version_limit_and_exact_expression() {
        assert_eq!(artist_query_key("Eve."), "v2|10|artist:\"Eve.\"");
        assert_ne!(artist_query_key("Eve."), artist_query_key("Eve"));
    }

    #[test]
    fn isrc_normalisation() {
        assert_eq!(normalize_isrc(" gb-aaa-20-12345 ").as_deref(), Some("GBAAA2012345"));
        assert_eq!(normalize_isrc("  "), None);
    }

    // ----- containment ----------------------------------------------------

    #[test]
    fn swim_deep_window_never_resolves_to_deep_purple() {
        let window = swim_deep_window();
        // Score 88 < 90: the containment rule yields nothing — and NOT the
        // unrelated first result.
        assert_eq!(select_exact_name("Swim Deep", &window), ExactNameSelection::None);
        assert_eq!(decide_source_identity(&[], &[]), SourceIdentity::NotFound);
    }

    #[test]
    fn quoted_window_resolves_swim_deep_uniquely() {
        let window = vec![cand(SWIM_DEEP, "Swim Deep", 100)];
        match select_exact_name("swim deep", &window) {
            ExactNameSelection::Unique(c) => assert_eq!(c.mbid, SWIM_DEEP),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn low_score_unrelated_first_result_is_never_accepted() {
        let window = vec![cand("x", "Something Else", 100), cand("y", "Swim Deep", 60)];
        assert_eq!(select_exact_name("Swim Deep", &window), ExactNameSelection::None);
    }

    #[test]
    fn several_exact_names_are_ambiguous_not_best_of_ties() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95)];
        assert_eq!(select_exact_name("Eve", &window), ExactNameSelection::Ambiguous(2));
        assert!(matches!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &[]),
            SourceIdentity::Ambiguous
        ));
    }

    // ----- ISRC decisions -------------------------------------------------

    #[test]
    fn one_isrc_with_one_exact_candidate_in_credits_resolves() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95)];
        let ev = vec![credits("GBAAA0000001", &[&["a", "guest-z"]])];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::VerifiedIsrc {
                mbid: "a".into(),
                isrc_votes: 1
            }
        );
    }

    #[test]
    fn collaboration_credited_to_two_homonyms_stays_unresolved() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95)];
        let ev = vec![credits("GBAAA0000001", &[&["a", "b"]])];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::Ambiguous
        );
    }

    #[test]
    fn collaboration_does_not_select_its_first_credit() {
        // The expected artist is the SECOND credit; the first credit is a
        // homonym-free stranger who is not a name candidate at all.
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95)];
        let ev = vec![credits("GBAAA0000001", &[&["stranger", "b"]])];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::VerifiedIsrc {
                mbid: "b".into(),
                isrc_votes: 1
            }
        );
    }

    #[test]
    fn two_distinct_isrcs_agreeing_resolve_over_collaborations() {
        // No single ISRC is unique, but "a" appears on two distinct ISRCs
        // and every other candidate on one.
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95), cand("c", "Eve", 92)];
        let ev = vec![
            credits("GBAAA0000001", &[&["a", "b"]]),
            credits("GBAAA0000002", &[&["a", "c"]]),
        ];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::VerifiedIsrc {
                mbid: "a".into(),
                isrc_votes: 2
            }
        );
    }

    #[test]
    fn duplicate_copies_of_one_isrc_count_once() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95), cand("c", "Eve", 92)];
        // The same ISRC twice (two recording rows AND two evidence rows).
        let ev = vec![
            credits("GBAAA0000001", &[&["a", "b"], &["a", "b"]]),
            credits("gbaaa0000001", &[&["a", "b"]]),
        ];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::Ambiguous
        );
    }

    #[test]
    fn conflicting_unique_votes_fail_closed() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95)];
        let ev = vec![
            credits("GBAAA0000001", &[&["a"]]),
            credits("GBAAA0000002", &[&["b"]]),
        ];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::Ambiguous
        );
    }

    #[test]
    fn a_unique_vote_contradicted_by_a_more_credited_rival_fails_closed() {
        let window = vec![cand("a", "Eve", 100), cand("b", "Eve", 95), cand("c", "Eve", 92)];
        let ev = vec![
            credits("GBAAA0000001", &[&["a"]]),
            credits("GBAAA0000002", &[&["b", "c"]]),
            credits("GBAAA0000003", &[&["b", "c"]]),
        ];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &window), &ev),
            SourceIdentity::Ambiguous
        );
    }

    #[test]
    fn missing_isrc_evidence_falls_back_to_the_name_rule_only() {
        let one = vec![cand("a", "Eve", 100)];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &one), &[]),
            SourceIdentity::UniqueExactName {
                mbid: "a".into(),
                score: Some(100)
            }
        );
        // An ISRC credited to nobody in the window is no evidence either way.
        let ev = vec![credits("GBAAA0000001", &[&["stranger"]])];
        assert_eq!(
            decide_source_identity(&exact_name_candidates("Eve", &one), &ev),
            SourceIdentity::UniqueExactName {
                mbid: "a".into(),
                score: Some(100)
            }
        );
    }

    #[test]
    fn recording_without_the_queried_isrc_contributes_no_vote() {
        let response: RecordingSearchResponse = serde_json::from_value(serde_json::json!({
            "count": 2, "offset": 0,
            "recordings": [
                { "id": "r1", "score": 100, "isrcs": ["GBAAA0000009"],
                  "artist-credit": [ { "artist": { "id": "stranger", "name": "S" } } ] },
                { "id": "r2", "score": 100, "isrcs": ["gbaaa0000001"],
                  "artist-credit": [ { "artist": { "id": "a", "name": "Eve" } } ] }
            ]
        }))
        .unwrap();
        let ev = IsrcCredits::from_search("GBAAA0000001", &response);
        assert_eq!(ev.isrc, "GBAAA0000001");
        assert_eq!(ev.recordings.len(), 1, "only the recording that carries the ISRC");
        assert_eq!(ev.recordings[0].recording_mbid, "r2");
        assert!(ev.credited().contains("a"));
        assert!(!ev.credited().contains("stranger"));
    }

    #[test]
    fn guest_top_tracks_are_excluded_and_isrcs_deduped_and_capped() {
        let tracks = vec![
            (Some("GBAAA0000001"), Some(7)),
            (Some("GBAAA0000002"), Some(99)), // guest appearance
            (Some("gb-aaa-00-00001"), Some(7)), // same ISRC, other spelling
            (None, Some(7)),
            (Some("GBAAA0000003"), Some(7)),
            (Some("GBAAA0000004"), Some(7)),
            (Some("GBAAA0000005"), Some(7)),
        ];
        let picked = pick_source_isrcs(7, tracks, MAX_SOURCE_ISRCS);
        assert_eq!(picked, vec!["GBAAA0000001", "GBAAA0000003", "GBAAA0000004"]);
    }

    // ----- scene decisions ------------------------------------------------

    #[test]
    fn scene_picks_the_duplicate_whose_isrcs_credit_the_expected_mbid() {
        let ev = vec![
            (1, Some(40), vec![credits("A1", &[&["other"]])]),
            (2, Some(3), vec![credits("B1", &[&["x", "mb-expected"]])]),
        ];
        assert_eq!(
            decide_scene_candidate("mb-expected", &ev),
            SceneDecision::Verified {
                qobuz_id: 2,
                isrc_votes: 1
            }
        );
    }

    #[test]
    fn scene_with_no_verified_duplicate_is_unresolved_not_max_albums() {
        let ev = vec![
            (1, Some(40), vec![credits("A1", &[&["other"]])]),
            (2, Some(3), vec![]),
        ];
        assert_eq!(decide_scene_candidate("mb-expected", &ev), SceneDecision::Unresolved);
    }

    #[test]
    fn scene_two_verified_duplicates_use_albums_count_then_tie_fails() {
        let ev = vec![
            (1, Some(40), vec![credits("A1", &[&["mb-expected"]])]),
            (2, Some(3), vec![credits("B1", &[&["mb-expected"]])]),
        ];
        assert_eq!(
            decide_scene_candidate("mb-expected", &ev),
            SceneDecision::Verified {
                qobuz_id: 1,
                isrc_votes: 1
            }
        );
        let tie = vec![
            (1, Some(3), vec![credits("A1", &[&["mb-expected"]])]),
            (2, Some(3), vec![credits("B1", &[&["mb-expected"]])]),
        ];
        assert_eq!(decide_scene_candidate("mb-expected", &tie), SceneDecision::Unresolved);
    }

    // ----- orchestration with fakes (request counting) ---------------------

    #[derive(Default)]
    struct FakeFetcher {
        windows: Mutex<std::collections::HashMap<String, Vec<ArtistCandidate>>>,
        isrcs: Mutex<std::collections::HashMap<String, IsrcCredits>>,
        fail_isrcs: bool,
        fail_search: bool,
        name_requests: Mutex<usize>,
        isrc_requests: Mutex<usize>,
    }

    impl FakeFetcher {
        fn with_window(self, name: &str, window: Vec<ArtistCandidate>) -> Self {
            self.windows.lock().unwrap().insert(name.to_string(), window);
            self
        }
        fn with_isrc(self, c: IsrcCredits) -> Self {
            self.isrcs.lock().unwrap().insert(c.isrc.clone(), c);
            self
        }
        fn counts(&self) -> (usize, usize) {
            (*self.name_requests.lock().unwrap(), *self.isrc_requests.lock().unwrap())
        }
    }

    impl IdentityFetcher for FakeFetcher {
        fn artist_candidates<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<Vec<ArtistCandidate>, FetchError>> {
            Box::pin(async move {
                *self.name_requests.lock().unwrap() += 1;
                if self.fail_search {
                    return Err(FetchError::Unavailable("503".into()));
                }
                Ok(self.windows.lock().unwrap().get(name).cloned().unwrap_or_default())
            })
        }
        fn isrc_credits<'a>(&'a self, isrc: &'a str) -> BoxFuture<'a, Result<IsrcCredits, FetchError>> {
            Box::pin(async move {
                *self.isrc_requests.lock().unwrap() += 1;
                if self.fail_isrcs {
                    return Err(FetchError::Unavailable("503".into()));
                }
                Ok(self
                    .isrcs
                    .lock()
                    .unwrap()
                    .get(isrc)
                    .cloned()
                    .unwrap_or(IsrcCredits {
                        isrc: isrc.to_string(),
                        recordings: vec![],
                    }))
            })
        }
    }

    #[derive(Default)]
    struct MemStore {
        queries: Mutex<std::collections::HashMap<String, Vec<ArtistCandidate>>>,
        isrcs: Mutex<std::collections::HashMap<String, IsrcCredits>>,
        sources: Mutex<std::collections::HashMap<u64, SourceIdentityRow>>,
        scenes: Mutex<std::collections::HashMap<(String, String), SceneIdentityRow>>,
    }

    impl IdentityStore for MemStore {
        fn artist_query(&self, key: &str) -> Option<Vec<ArtistCandidate>> {
            self.queries.lock().unwrap().get(key).cloned()
        }
        fn put_artist_query(&self, key: &str, window: &[ArtistCandidate]) {
            self.queries.lock().unwrap().insert(key.into(), window.to_vec());
        }
        fn isrc_credits(&self, isrc: &str) -> Option<IsrcCredits> {
            self.isrcs.lock().unwrap().get(isrc).cloned()
        }
        fn put_isrc_credits(&self, credits: &IsrcCredits) {
            self.isrcs.lock().unwrap().insert(credits.isrc.clone(), credits.clone());
        }
        fn source_identity(&self, qobuz_artist_id: u64) -> Option<SourceIdentityRow> {
            self.sources
                .lock()
                .unwrap()
                .get(&qobuz_artist_id)
                .filter(|r| r.evidence_kind == EvidenceKind::VerifiedIsrc)
                .cloned()
        }
        fn put_source_identity(&self, row: &SourceIdentityRow) {
            self.sources.lock().unwrap().insert(row.qobuz_artist_id, row.clone());
        }
        fn scene_identity(&self, mbid: &str, scope: &str) -> Option<SceneIdentityRow> {
            self.scenes.lock().unwrap().get(&(mbid.into(), scope.into())).cloned()
        }
        fn put_scene_identity(&self, row: &SceneIdentityRow) {
            self.scenes
                .lock()
                .unwrap()
                .insert((row.mbid.clone(), row.scope.clone()), row.clone());
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().build().unwrap()
    }

    #[test]
    fn source_resolution_is_bounded_to_the_distinct_isrc_cap() {
        let fetcher = FakeFetcher::default()
            .with_window("Eve", vec![cand("a", "Eve", 100), cand("b", "Eve", 95)])
            .with_isrc(credits("I1", &[&["a", "b"]]))
            .with_isrc(credits("I2", &[&["a", "b"]]))
            .with_isrc(credits("I3", &[&["a", "b"]]));
        let store = MemStore::default();
        let isrcs: Vec<String> = ["I1", "i1", "I2", "I3", "I4", "I5"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &isrcs));
        assert_eq!(fetcher.counts(), (1, MAX_SOURCE_ISRCS));
        assert_eq!(out.requests, 1 + MAX_SOURCE_ISRCS);
        assert_eq!(out.identity, SourceIdentity::Ambiguous);
        assert!(store.sources.lock().unwrap().is_empty(), "ambiguous writes no identity");
    }

    #[test]
    fn verified_source_is_cached_by_qobuz_id_and_replays_with_zero_requests() {
        let fetcher = FakeFetcher::default()
            .with_window("Eve", vec![cand("a", "Eve", 100), cand("b", "Eve", 95)])
            .with_isrc(credits("I1", &[&["a"]]));
        let store = MemStore::default();
        let isrcs = vec!["I1".to_string()];
        let first = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &isrcs));
        assert!(matches!(first.identity, SourceIdentity::VerifiedIsrc { .. }));
        assert_eq!(first.resolved.as_ref().map(|r| r.mbid.as_str()), Some("a"));

        // Same name, OTHER Qobuz id: no shared mapping.
        assert!(store.source_identity(8).is_none());

        let again = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &isrcs));
        assert_eq!(again.cache, "hit");
        assert_eq!(again.requests, 0);
        assert_eq!(fetcher.counts(), (1, 1), "second resolve issued nothing");
    }

    #[test]
    fn containment_only_result_writes_no_source_identity() {
        let fetcher = FakeFetcher::default().with_window("Eve", vec![cand("a", "Eve", 100)]);
        let store = MemStore::default();
        let out = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &[]));
        assert!(matches!(out.identity, SourceIdentity::UniqueExactName { .. }));
        assert!(store.sources.lock().unwrap().is_empty());
        // ...but the search WINDOW is cached as evidence and re-gated.
        assert!(store.artist_query(&artist_query_key("Eve")).is_some());
        let again = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &[]));
        assert_eq!(again.requests, 0);
    }

    #[test]
    fn search_failure_is_unavailable_and_cached_nowhere() {
        let fetcher = FakeFetcher {
            fail_search: true,
            ..Default::default()
        };
        let store = MemStore::default();
        let out = rt().block_on(resolve_source_artist(Some(&store), &fetcher, 7, "Eve", &[]));
        assert_eq!(out.identity, SourceIdentity::Unavailable);
        assert!(store.queries.lock().unwrap().is_empty());
        assert!(store.sources.lock().unwrap().is_empty());
    }

    #[test]
    fn isrc_failure_with_ambiguous_name_is_unavailable_not_negative() {
        let fetcher = FakeFetcher {
            fail_isrcs: true,
            ..Default::default()
        }
        .with_window("Eve", vec![cand("a", "Eve", 100), cand("b", "Eve", 95)]);
        let store = MemStore::default();
        let out = rt().block_on(resolve_source_artist(
            Some(&store),
            &fetcher,
            7,
            "Eve",
            &["I1".to_string()],
        ));
        assert_eq!(out.identity, SourceIdentity::Unavailable);
        assert!(store.sources.lock().unwrap().is_empty());
        assert!(store.isrcs.lock().unwrap().is_empty());
    }

    #[test]
    fn name_resolver_never_returns_an_unrelated_first_result() {
        let fetcher = FakeFetcher::default().with_window("Swim Deep", swim_deep_window());
        let out = rt()
            .block_on(resolve_artist_by_name(None, &fetcher, "Swim Deep"))
            .unwrap();
        assert!(out.is_none());
    }

    #[derive(Default)]
    struct FakeCatalog {
        by_name: std::collections::HashMap<String, Vec<SceneQobuzCandidate>>,
        isrcs: std::collections::HashMap<u64, Vec<String>>,
        page_requests: Mutex<usize>,
    }

    impl SceneCatalog for FakeCatalog {
        fn exact_name_candidates<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<Vec<SceneQobuzCandidate>, FetchError>> {
            Box::pin(async move { Ok(self.by_name.get(name).cloned().unwrap_or_default()) })
        }
        fn candidate_isrcs<'a>(
            &'a self,
            qobuz_id: u64,
            cap: usize,
        ) -> BoxFuture<'a, Result<Vec<String>, FetchError>> {
            Box::pin(async move {
                *self.page_requests.lock().unwrap() += 1;
                Ok(self
                    .isrcs
                    .get(&qobuz_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .take(cap)
                    .collect())
            })
        }
    }

    fn qc(id: u64, name: &str, albums: u32) -> SceneQobuzCandidate {
        SceneQobuzCandidate {
            qobuz_id: id,
            name: name.into(),
            image: None,
            albums_count: Some(albums),
        }
    }

    #[test]
    fn unique_scene_name_performs_no_isrc_fan_out_and_is_provisional() {
        let mut catalog = FakeCatalog::default();
        catalog.by_name.insert("Eve".into(), vec![qc(1, "Eve", 4)]);
        let fetcher = FakeFetcher::default();
        let store = MemStore::default();
        let out = rt()
            .block_on(resolve_scene_candidate(
                Some(&store),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                Some("FR"),
                &|_| false,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(out.qobuz_id, 1);
        assert_eq!(out.kind, EvidenceKind::UniqueExactName);
        assert_eq!(fetcher.counts(), (0, 0));
        assert_eq!(*catalog.page_requests.lock().unwrap(), 0);
        let row = store.scene_identity("mb-1", "FR").unwrap();
        assert_eq!(row.evidence_kind, EvidenceKind::UniqueExactName);
        // Same name, other MBID: no shared mapping.
        assert!(store.scene_identity("mb-2", "FR").is_none());
        // Other scope: no shared mapping either.
        assert!(store.scene_identity("mb-1", "US").is_none());
    }

    #[test]
    fn ambiguous_scene_name_is_bounded_and_picks_the_verified_duplicate() {
        let mut catalog = FakeCatalog::default();
        catalog
            .by_name
            .insert("Eve".into(), vec![qc(1, "Eve", 40), qc(2, "Eve", 3)]);
        catalog.isrcs.insert(1, vec!["A1".into(), "A2".into(), "A3".into()]);
        catalog.isrcs.insert(2, vec!["B1".into(), "B2".into(), "B3".into()]);
        let fetcher = FakeFetcher::default()
            .with_isrc(credits("A1", &[&["other"]]))
            .with_isrc(credits("A2", &[&["other"]]))
            .with_isrc(credits("B1", &[&["mb-1"]]))
            .with_isrc(credits("B2", &[&["mb-1"]]));
        let store = MemStore::default();
        let out = rt()
            .block_on(resolve_scene_candidate(
                Some(&store),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                Some("FR"),
                &|_| false,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(out.qobuz_id, 2, "the smaller catalog wins because it is the verified one");
        assert_eq!(out.kind, EvidenceKind::VerifiedIsrc);
        assert_eq!(fetcher.counts(), (0, 2 * MAX_SCENE_ISRCS_PER_CANDIDATE));
        assert_eq!(*catalog.page_requests.lock().unwrap(), 2);
        assert_eq!(
            store.scene_identity("mb-1", "FR").unwrap().evidence_kind,
            EvidenceKind::VerifiedIsrc
        );
        // Replay: zero requests.
        let again = rt()
            .block_on(resolve_scene_candidate(
                Some(&store),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                Some("FR"),
                &|_| false,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(again.requests, 0);
        assert_eq!(fetcher.counts(), (0, 2 * MAX_SCENE_ISRCS_PER_CANDIDATE));
    }

    #[test]
    fn ambiguous_scene_name_without_evidence_omits_the_row() {
        let mut catalog = FakeCatalog::default();
        catalog
            .by_name
            .insert("Eve".into(), vec![qc(1, "Eve", 40), qc(2, "Eve", 3)]);
        let fetcher = FakeFetcher::default();
        let store = MemStore::default();
        let out = rt()
            .block_on(resolve_scene_candidate(
                Some(&store),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                Some("FR"),
                &|_| false,
            ))
            .unwrap();
        assert!(out.is_none(), "never the most prolific unverified homonym");
        assert!(store.scenes.lock().unwrap().is_empty());
    }

    #[test]
    fn blacklisted_scene_hit_is_a_miss_and_unknown_scope_persists_nothing() {
        let mut catalog = FakeCatalog::default();
        catalog.by_name.insert("Eve".into(), vec![qc(1, "Eve", 4), qc(9, "Eve", 1)]);
        let fetcher = FakeFetcher::default();
        let store = MemStore::default();
        store.put_scene_identity(&SceneIdentityRow {
            mbid: "mb-1".into(),
            scope: "FR".into(),
            qobuz_id: 9,
            evidence_kind: EvidenceKind::VerifiedIsrc,
            evidence_count: 1,
            qobuz_name: "Eve".into(),
            image: None,
            albums_count: Some(1),
        });
        let blocked = |id: u64| id == 9;
        // Hit on 9 is blacklisted -> miss -> fresh path: 1 remains unique.
        let out = rt()
            .block_on(resolve_scene_candidate(
                Some(&store),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                Some("FR"),
                &blocked,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(out.qobuz_id, 1);

        let store2 = MemStore::default();
        let _ = rt()
            .block_on(resolve_scene_candidate(
                Some(&store2),
                &fetcher,
                &catalog,
                "mb-1",
                "Eve",
                None,
                &|_| false,
            ))
            .unwrap();
        assert!(store2.scenes.lock().unwrap().is_empty(), "no scope, no durable row");
    }
}
