//! Pure read-cache algorithms over caller-owned lines.
//!
//! The library computes validity, admission and coverage; the daemon owns
//! the live 128-byte lines, their storage, the epoch counter and every
//! fill/evict. A line is keyed by `(address, bank, page, half)` with a
//! valid sub-range, records the epoch it was filled in, and may hold only
//! session-static bytes. There is no transport-owning `CachedReader` here
//! and no time-based reuse of dynamic data — freshness is per observation
//! and validity is per epoch, never a TTL.
//!
//! Still owed by task 39's full contract: eviction/capacity decisions,
//! pin/settle/refresh ownership and refresh-completion application, and
//! the admission/publication delta plumbing; nothing here pre-empts them.

use super::{ByteClassification, CacheEpoch, ChangeSource, Range};

/// Which 128-byte half of a page one line holds (0 = bytes 0..128,
/// 1 = bytes 128..256). The key the daemon's line store uses, kept as
/// plain data so the library can reason about coverage without owning
/// storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Seven-bit I2C address of the line's page.
    pub address: u8,
    /// Bank selector of the line's page.
    pub bank: u8,
    /// Page selector of the line's page.
    pub page: u8,
    /// Which 128-byte half of the page (0 or 1).
    pub half: u8,
}

/// One caller-owned cache line's metadata, as supplied per call. The bytes
/// themselves stay in the daemon's store; only the validity facts travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLine {
    /// The line's key (which page and half it holds).
    pub key: CacheKey,
    /// First page-relative byte offset the line holds valid data for.
    pub valid_from: u16,
    /// Number of valid bytes starting at `valid_from`.
    pub valid_len: u16,
    /// The epoch the line was filled in. A line from an older epoch is a
    /// miss and the owner drops it.
    pub epoch: CacheEpoch,
    /// The owner's pin: a pinned line is never an eviction victim, and a
    /// fill that would need it is refused rather than evicting it.
    pub pinned: bool,
}

impl CacheLine {
    /// The page-relative range this line's valid span covers, as a
    /// [`Range`] against the line's own key.
    pub fn valid_range(&self) -> Range {
        Range {
            address: self.key.address,
            bank: self.key.bank,
            page: self.key.page,
            offset: self.valid_from as u8,
            len: self.valid_len,
        }
    }
}

/// Why a lookup missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMiss {
    /// No line with the requested key exists.
    Absent,
    /// A line exists but was filled in an older epoch: a miss, and the
    /// owner must drop it (it can never serve cross-epoch).
    /// A line exists but was filled in an older epoch: a miss, and the
    /// owner must drop it (it can never serve cross-epoch).
    StaleEpoch {
        /// The epoch the droppable line was filled in.
        line_epoch: CacheEpoch,
    },
    /// A current-epoch line exists but its valid span does not cover the
    /// requested range.
    Uncovered {
        /// The epoch of the line that failed to cover.
        line_epoch: CacheEpoch,
    },
}

/// The result of one pure cache lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLookup<'a> {
    /// The requested range is served by this line's valid span. The bytes
    /// come from the caller's store; the hit proves only validity.
    Hit {
        /// The line that covers the requested range.
        line: &'a CacheLine,
    },
    /// The lookup missed for the named reason.
    Miss(CacheMiss),
}

/// Look up `wanted` (a page-relative range against `key`'s address, bank
/// and page) among the supplied lines, against the caller's current epoch.
///
/// Pure: nothing is filled, evicted or refreshed. A line from an older
/// epoch is a [`CacheMiss::StaleEpoch`] miss even when it covers the range
/// — the caller learns the line is droppable, never that it is servable.
pub fn cache_lookup<'a>(
    lines: &'a [CacheLine],
    key: CacheKey,
    wanted: &Range,
    current_epoch: CacheEpoch,
) -> CacheLookup<'a> {
    let Some(line) = lines.iter().find(|line| line.key == key) else {
        return CacheLookup::Miss(CacheMiss::Absent);
    };
    if line.epoch != current_epoch {
        return CacheLookup::Miss(CacheMiss::StaleEpoch {
            line_epoch: line.epoch,
        });
    }
    // Page-relative coverage: the line's valid span must contain the
    // requested span within the same half.
    let wanted_from = (wanted.offset as u16) + u16::from(key.half) * 128;
    let wanted_to = wanted_from + wanted.len;
    let valid_to = line.valid_from + line.valid_len;
    if wanted_from >= line.valid_from && wanted_to <= valid_to {
        CacheLookup::Hit { line }
    } else {
        CacheLookup::Miss(CacheMiss::Uncovered {
            line_epoch: line.epoch,
        })
    }
}

/// What the eviction decision says the caller should do. The caller owns
/// the store: it applies the eviction, never the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictDecision {
    /// Evict this line to make room: the victim is unpinned, is not one of
    /// the keys the caller must keep, and is the best victim by the rule
    /// below.
    Evict {
        /// The key of the line to evict.
        key: CacheKey,
    },
    /// No victim exists: every candidate is pinned or must be kept. The
    /// caller refuses the fill rather than evicting a pinned line — fail
    /// closed, never a silent unpin.
    Refuse,
}

/// Choose an eviction victim among the caller-owned lines (task 39 slice).
///
/// Preference, in order: (1) a line from an older epoch — it can never
/// serve cross-epoch, so it is a strictly better victim than a
/// current-epoch line the cache could still hit; (2) the lowest key
/// (address, bank, page, half) for determinism — recency is the
/// owner's fact, and this reducer does not invent it. A pinned line is
/// never a victim, and neither is a key in `keep`. Pure: nothing is
/// dropped here; the decision names the victim.
pub fn evict_victim(
    lines: &[CacheLine],
    keep: &[CacheKey],
    current_epoch: CacheEpoch,
) -> EvictDecision {
    // Sort key: stale-epoch lines first (they can never serve again),
    // then the lowest epoch, then the lowest key.
    let rank = |line: &CacheLine| {
        let key = (
            line.key.address,
            line.key.bank,
            line.key.page,
            line.key.half,
        );
        (line.epoch == current_epoch, line.epoch, key)
    };
    let mut best: Option<&CacheLine> = None;
    for line in lines {
        if line.pinned || keep.contains(&line.key) {
            continue;
        }
        let better = match best {
            None => true,
            Some(current) => rank(line) < rank(current),
        };
        if better {
            best = Some(line);
        }
    }
    match best {
        Some(line) => EvictDecision::Evict { key: line.key },
        None => EvictDecision::Refuse,
    }
}

/// What the fill/refresh plan says the caller should do about a wanted
/// range, as data. The caller owns every fill, drop and read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheFillDecision {
    /// The wanted range is served by a current-epoch line: nothing to do.
    Served,
    /// No line with this key exists: fill the wanted range.
    Fill {
        /// The range to fill.
        wanted: Range,
    },
    /// A line exists but is from an older epoch: it can never serve
    /// cross-epoch, so drop it and refill the wanted range fresh.
    DropAndRefill {
        /// The key of the line to drop.
        drop: CacheKey,
        /// The range to refill.
        wanted: Range,
    },
    /// A current-epoch line exists but does not cover the wanted span:
    /// keep it and fill only the uncovered sub-ranges, which are returned
    /// in ascending offset order.
    PartialRefill {
        /// The key of the line to keep.
        keep: CacheKey,
        /// The uncovered sub-ranges to fill, ascending by offset.
        uncovered: Vec<Range>,
    },
}

/// Plan what to do about a wanted range after a [`cache_lookup`] (task 39
/// slice: refresh decisions over caller-owned entries). Pure: the plan is
/// data; the caller reads, drops and fills through the store it owns.
///
/// A stale-epoch line is a drop-AND-refill, never a serve; an uncovered
/// current-epoch line is kept and only its uncovered sub-ranges are
/// filled — a conservative, complete residual: the prefix the line
/// starts too late for, then the suffix it ends too early for.
pub fn fill_plan(
    lines: &[CacheLine],
    key: CacheKey,
    wanted: &Range,
    current_epoch: CacheEpoch,
) -> CacheFillDecision {
    match cache_lookup(lines, key, wanted, current_epoch) {
        CacheLookup::Hit { .. } => CacheFillDecision::Served,
        CacheLookup::Miss(CacheMiss::Absent) => CacheFillDecision::Fill { wanted: *wanted },
        CacheLookup::Miss(CacheMiss::StaleEpoch { line_epoch }) => {
            let _ = line_epoch;
            CacheFillDecision::DropAndRefill {
                drop: key,
                wanted: *wanted,
            }
        }
        CacheLookup::Miss(CacheMiss::Uncovered { .. }) => {
            let line = lines
                .iter()
                .find(|line| line.key == key)
                .expect("an uncovered miss implies the line exists");
            let wanted_from = u16::from(wanted.offset) + u16::from(key.half) * 128;
            let wanted_to = wanted_from + wanted.len;
            let valid_from = line.valid_from;
            let valid_to = line.valid_from + line.valid_len;
            let mut uncovered: Vec<Range> = Vec::new();
            let range_at = |start: u16, end: u16| Range {
                address: key.address,
                bank: key.bank,
                page: key.page,
                offset: start as u8,
                len: end - start,
            };
            if wanted_from < valid_from {
                uncovered.push(range_at(wanted_from, valid_from));
            }
            if wanted_to > valid_to {
                uncovered.push(range_at(valid_to, wanted_to));
            }
            CacheFillDecision::PartialRefill {
                keep: key,
                uncovered,
            }
        }
    }
}

/// Whether a candidate line may be filled at all: every byte must be
/// explicitly side-effect-free readable (`qualified None`) and static.
///
/// D5 cache rule: lines are filled only with session-static bytes. A
/// single dynamic, unknown, write-only or uncovered constituent byte
/// refuses the whole line — conservative, never a partial fill. The
/// classifications are the generated schema metadata the caller already
/// holds; nothing is read here.
pub fn line_fillable(classifications: &[ByteClassification]) -> bool {
    classifications.len() == 128
        && classifications
            .iter()
            .all(|byte| byte.is_qualified_none() && byte.change_source == ChangeSource::Static)
}

/// The cache lines a planned write invalidates (task 39/64: covered-line
/// invalidation). Pure: returns the keys of caller-owned lines whose page
/// overlaps the written range — those bytes are stale the moment the write
/// may have applied, confirmed or not, so the owner drops them and refills
/// from the next observation. A write outside every line's page touches
/// nothing. Full invalidation (identity/password/CDB/reset/power/profile/
/// generation/boundary events) is the epoch-advance path, not this one:
/// the plan reducers return `AdvanceCacheEpoch` for those, and the owner
/// applies it — never one blanket rule for both.
pub fn write_invalidations(lines: &[CacheLine], written: &Range) -> Vec<CacheKey> {
    let mut keys = Vec::new();
    for line in lines {
        let same_page = line.key.address == written.address
            && line.key.bank == written.bank
            && line.key.page == written.page;
        if same_page {
            keys.push(line.key);
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: CacheKey = CacheKey {
        address: 0x50,
        bank: 0,
        page: 0x11,
        half: 0,
    };
    const EPOCH: CacheEpoch = CacheEpoch(3);

    fn full_line() -> CacheLine {
        CacheLine {
            key: KEY,
            valid_from: 0,
            valid_len: 128,
            epoch: EPOCH,
            pinned: false,
        }
    }

    fn range(offset: u8, len: u16) -> Range {
        Range {
            address: KEY.address,
            bank: KEY.bank,
            page: KEY.page,
            offset,
            len,
        }
    }

    #[test]
    fn a_current_epoch_covering_line_hits() {
        let lines = [full_line()];
        assert_eq!(
            cache_lookup(&lines, KEY, &range(0, 128), EPOCH),
            CacheLookup::Hit { line: &lines[0] }
        );
        assert_eq!(
            cache_lookup(&lines, KEY, &range(10, 5), EPOCH),
            CacheLookup::Hit { line: &lines[0] }
        );
    }

    #[test]
    fn a_partial_valid_span_covers_only_within_itself() {
        let mut line = full_line();
        line.valid_from = 16;
        line.valid_len = 8;
        let lines = [line];
        assert_eq!(
            cache_lookup(&lines, KEY, &range(16, 8), EPOCH),
            CacheLookup::Hit { line: &lines[0] }
        );
        assert_eq!(
            cache_lookup(&lines, KEY, &range(16, 9), EPOCH),
            CacheLookup::Miss(CacheMiss::Uncovered { line_epoch: EPOCH })
        );
        assert_eq!(
            cache_lookup(&lines, KEY, &range(15, 2), EPOCH),
            CacheLookup::Miss(CacheMiss::Uncovered { line_epoch: EPOCH })
        );
    }

    #[test]
    fn an_older_epoch_line_is_a_stale_miss_even_when_it_covers() {
        let mut line = full_line();
        line.epoch = CacheEpoch(EPOCH.0 - 1);
        let lines = [line];
        assert_eq!(
            cache_lookup(&lines, KEY, &range(0, 128), EPOCH),
            CacheLookup::Miss(CacheMiss::StaleEpoch {
                line_epoch: CacheEpoch(EPOCH.0 - 1)
            })
        );
    }

    #[test]
    fn a_missing_line_is_an_absent_miss() {
        let lines = [full_line()];
        let other = CacheKey { page: 0x01, ..KEY };
        assert_eq!(
            cache_lookup(&lines, other, &range(0, 1), EPOCH),
            CacheLookup::Miss(CacheMiss::Absent)
        );
        assert_eq!(
            cache_lookup(&[], KEY, &range(0, 1), EPOCH),
            CacheLookup::Miss(CacheMiss::Absent)
        );
    }

    #[test]
    fn eviction_prefers_stale_epochs_then_lowest_keys_and_never_pinned() {
        let mut stale = full_line();
        stale.epoch = CacheEpoch(EPOCH.0 - 1);
        stale.key.page = 0x02;
        let mut pinned_low = full_line();
        pinned_low.key.page = 0x01;
        pinned_low.pinned = true;
        let mut current = full_line();
        current.key.page = 0x03;
        let lines = [stale, pinned_low, current];
        // The stale-epoch line is the best victim even though its key is
        // not the lowest; the pinned line is never a victim.
        assert_eq!(
            evict_victim(&lines, &[], EPOCH),
            EvictDecision::Evict {
                key: CacheKey { page: 0x02, ..KEY }
            }
        );
        // Without the stale line, the lowest unpinned key wins; the pinned
        // line is skipped even though its key is lower.
        assert_eq!(
            evict_victim(&[pinned_low, current], &[], EPOCH),
            EvictDecision::Evict {
                key: CacheKey { page: 0x03, ..KEY }
            }
        );
        // Kept keys are never victims: everything kept or pinned refuses.
        assert_eq!(
            evict_victim(&lines, &[CacheKey { page: 0x02, ..KEY }], EPOCH),
            EvictDecision::Evict {
                key: CacheKey { page: 0x03, ..KEY }
            }
        );
        assert_eq!(
            evict_victim(&[pinned_low], &[], EPOCH),
            EvictDecision::Refuse
        );
        assert_eq!(evict_victim(&[], &[], EPOCH), EvictDecision::Refuse);
    }

    #[test]
    fn fill_plan_decides_drop_refill_and_partial_residuals() {
        // Served: a covering current-epoch line needs nothing.
        let lines = [full_line()];
        assert_eq!(
            fill_plan(&lines, KEY, &range(0, 128), EPOCH),
            CacheFillDecision::Served
        );
        // Absent: fill the wanted range.
        assert_eq!(
            fill_plan(&lines, CacheKey { page: 0x01, ..KEY }, &range(0, 4), EPOCH),
            CacheFillDecision::Fill {
                wanted: range(0, 4)
            }
        );
        // Stale epoch: drop and refill, never serve.
        let mut stale_line = full_line();
        stale_line.epoch = CacheEpoch(EPOCH.0 - 1);
        let stale_lines = [stale_line];
        assert_eq!(
            fill_plan(&stale_lines, KEY, &range(0, 128), EPOCH),
            CacheFillDecision::DropAndRefill {
                drop: KEY,
                wanted: range(0, 128),
            }
        );
        // Uncovered: keep the line, fill only the residuals — the prefix
        // the line starts too late for, then the suffix it ends too early
        // for, in ascending order.
        let mut partial = full_line();
        partial.valid_from = 16;
        partial.valid_len = 8; // covers 16..24
        let partial_lines = [partial];
        // Wanted 8..32 against a line covering 16..24: both residuals,
        // in ascending order.
        assert_eq!(
            fill_plan(&partial_lines, KEY, &range(8, 24), EPOCH),
            CacheFillDecision::PartialRefill {
                keep: KEY,
                uncovered: vec![range(8, 8), range(24, 8)],
            }
        );
        // Only a prefix residual when the wanted span ends inside the
        // line's valid span.
        assert_eq!(
            fill_plan(&partial_lines, KEY, &range(8, 16), EPOCH),
            CacheFillDecision::PartialRefill {
                keep: KEY,
                uncovered: vec![range(8, 8)],
            }
        );
        // Fully inside the valid span: served even though the line is
        // partial.
        assert_eq!(
            fill_plan(&partial_lines, KEY, &range(16, 8), EPOCH),
            CacheFillDecision::Served
        );
    }

    fn byte(qualified_none: bool, source: ChangeSource) -> ByteClassification {
        ByteClassification {
            known_effects: if qualified_none {
                vec![super::super::ReadEffect::None]
            } else {
                vec![]
            },
            readable_mask: if qualified_none { 0xff } else { 0 },
            unknown_mask: 0,
            uncovered_mask: 0,
            write_only_mask: 0,
            change_source: source,
        }
    }

    #[test]
    fn only_fully_static_side_effect_free_lines_are_fillable() {
        let all_static = vec![byte(true, ChangeSource::Static); 128];
        assert!(line_fillable(&all_static));
        // One dynamic byte refuses the whole line.
        let mut with_dynamic = all_static.clone();
        with_dynamic[7] = byte(true, ChangeSource::Dynamic);
        assert!(!line_fillable(&with_dynamic));
        // An unqualified (unknown-effect) byte refuses it too.
        let mut with_unknown = vec![byte(true, ChangeSource::Static); 128];
        with_unknown[7] = byte(false, ChangeSource::Static);
        assert!(!line_fillable(&with_unknown));
        // A short slice is not a line.
        assert!(!line_fillable(&all_static[..64]));
    }
}
