use crate::{TechnologyCategorySlug, TechnologySlug};
use std::collections::{BTreeMap, BTreeSet};

/// The outcome of evaluating one published rule against one snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleObservationStatus {
    Detected,
    ConfirmedAbsent,
    Unknown,
}

impl RuleObservationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::ConfirmedAbsent => "confirmed_absent",
            Self::Unknown => "unknown",
        }
    }
}

/// The immutable per-rule coverage result used to derive current state safely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleObservation {
    technology_slug: TechnologySlug,
    technology_category_slug: TechnologyCategorySlug,
    rule_slug: crate::RuleSlug,
    rule_version: crate::RuleVersion,
    status: RuleObservationStatus,
}

impl RuleObservation {
    pub fn new(
        technology_slug: TechnologySlug,
        technology_category_slug: TechnologyCategorySlug,
        rule_slug: crate::RuleSlug,
        rule_version: crate::RuleVersion,
        status: RuleObservationStatus,
    ) -> Self {
        Self {
            technology_slug,
            technology_category_slug,
            rule_slug,
            rule_version,
            status,
        }
    }

    pub fn technology_slug(&self) -> &TechnologySlug {
        &self.technology_slug
    }
    pub fn technology_category_slug(&self) -> &TechnologyCategorySlug {
        &self.technology_category_slug
    }
    pub fn rule_slug(&self) -> &crate::RuleSlug {
        &self.rule_slug
    }
    pub fn rule_version(&self) -> crate::RuleVersion {
        self.rule_version
    }
    pub fn status(&self) -> RuleObservationStatus {
        self.status
    }
}

/// Current technology state supplied to the pure projection transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentTechnology {
    technology_slug: TechnologySlug,
    technology_category_slug: TechnologyCategorySlug,
}

impl CurrentTechnology {
    pub fn new(
        technology_slug: TechnologySlug,
        technology_category_slug: TechnologyCategorySlug,
    ) -> Self {
        Self {
            technology_slug,
            technology_category_slug,
        }
    }

    pub fn technology_slug(&self) -> &TechnologySlug {
        &self.technology_slug
    }
    pub fn technology_category_slug(&self) -> &TechnologyCategorySlug {
        &self.technology_category_slug
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechnologyChangeKind {
    Added,
    Removed,
    Migrated,
}

impl TechnologyChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Migrated => "migrated",
        }
    }
}

/// A material derived change between the prior current state and a new snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechnologyChange {
    kind: TechnologyChangeKind,
    from: Option<CurrentTechnology>,
    to: Option<CurrentTechnology>,
}

impl TechnologyChange {
    pub fn kind(&self) -> TechnologyChangeKind {
        self.kind
    }
    pub fn from(&self) -> Option<&CurrentTechnology> {
        self.from.as_ref()
    }
    pub fn to(&self) -> Option<&CurrentTechnology> {
        self.to.as_ref()
    }
}

/// The projection operations needed to apply one snapshot without mutating observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionTransition {
    activate: Vec<CurrentTechnology>,
    deactivate: Vec<CurrentTechnology>,
    changes: Vec<TechnologyChange>,
}

impl ProjectionTransition {
    pub fn activate(&self) -> &[CurrentTechnology] {
        &self.activate
    }
    pub fn deactivate(&self) -> &[CurrentTechnology] {
        &self.deactivate
    }
    pub fn changes(&self) -> &[TechnologyChange] {
        &self.changes
    }
}

/// Derives the next current stack and its material history from immutable observations.
pub fn derive_projection(
    previous: &[CurrentTechnology],
    observations: &[RuleObservation],
) -> ProjectionTransition {
    let previous_by_slug = previous
        .iter()
        .cloned()
        .map(|state| (state.technology_slug().as_str().to_owned(), state))
        .collect::<BTreeMap<_, _>>();
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    let mut activate = Vec::new();
    let mut deactivate = Vec::new();

    for observation in observations {
        let state = CurrentTechnology::new(
            observation.technology_slug().clone(),
            observation.technology_category_slug().clone(),
        );
        let was_current = previous_by_slug.contains_key(state.technology_slug().as_str());
        match observation.status() {
            RuleObservationStatus::Detected => {
                activate.push(state.clone());
                if !was_current {
                    additions.push(state);
                }
            }
            RuleObservationStatus::ConfirmedAbsent if was_current => {
                removals.push(state.clone());
                deactivate.push(state);
            }
            RuleObservationStatus::ConfirmedAbsent | RuleObservationStatus::Unknown => {}
        }
    }

    let mut migrated_additions = BTreeSet::new();
    let mut migrated_removals = BTreeSet::new();
    let mut changes = Vec::new();
    for addition in &additions {
        let matching_removals = removals
            .iter()
            .filter(|removal| {
                removal.technology_category_slug() == addition.technology_category_slug()
            })
            .collect::<Vec<_>>();
        let matching_additions = additions
            .iter()
            .filter(|candidate| {
                candidate.technology_category_slug() == addition.technology_category_slug()
            })
            .count();
        if matching_removals.len() == 1 && matching_additions == 1 {
            let removal = matching_removals[0];
            migrated_additions.insert(addition.technology_slug().as_str().to_owned());
            migrated_removals.insert(removal.technology_slug().as_str().to_owned());
            changes.push(TechnologyChange {
                kind: TechnologyChangeKind::Migrated,
                from: Some(removal.clone()),
                to: Some(addition.clone()),
            });
        }
    }
    for addition in additions {
        if !migrated_additions.contains(addition.technology_slug().as_str()) {
            changes.push(TechnologyChange {
                kind: TechnologyChangeKind::Added,
                from: None,
                to: Some(addition),
            });
        }
    }
    for removal in removals {
        if !migrated_removals.contains(removal.technology_slug().as_str()) {
            changes.push(TechnologyChange {
                kind: TechnologyChangeKind::Removed,
                from: Some(removal),
                to: None,
            });
        }
    }
    changes.sort_by(|left, right| {
        left.kind()
            .as_str()
            .cmp(right.kind().as_str())
            .then_with(|| {
                left.to()
                    .or(left.from())
                    .map(|state| state.technology_slug().as_str())
                    .cmp(
                        &right
                            .to()
                            .or(right.from())
                            .map(|state| state.technology_slug().as_str()),
                    )
            })
    });
    activate.sort_by(|left, right| {
        left.technology_slug()
            .as_str()
            .cmp(right.technology_slug().as_str())
    });
    activate.dedup_by(|left, right| left.technology_slug() == right.technology_slug());
    deactivate.sort_by(|left, right| {
        left.technology_slug()
            .as_str()
            .cmp(right.technology_slug().as_str())
    });
    deactivate.dedup_by(|left, right| left.technology_slug() == right.technology_slug());
    ProjectionTransition {
        activate,
        deactivate,
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn technology(slug: &str, category: &str) -> CurrentTechnology {
        CurrentTechnology::new(
            TechnologySlug::parse(slug).expect("valid technology"),
            TechnologyCategorySlug::parse(category).expect("valid category"),
        )
    }

    fn observation(slug: &str, category: &str, status: RuleObservationStatus) -> RuleObservation {
        RuleObservation::new(
            TechnologySlug::parse(slug).expect("valid technology"),
            TechnologyCategorySlug::parse(category).expect("valid category"),
            crate::RuleSlug::parse(&format!("{slug}-v1")).expect("valid rule"),
            crate::RuleVersion::new(1).expect("valid version"),
            status,
        )
    }

    #[test]
    fn derives_additions_removals_and_coverage_gated_migrations() {
        let first = derive_projection(
            &[],
            &[observation(
                "nextjs",
                "framework",
                RuleObservationStatus::Detected,
            )],
        );
        assert_eq!(first.changes().len(), 1);
        assert_eq!(first.changes()[0].kind(), TechnologyChangeKind::Added);

        let unknown = derive_projection(
            &[technology("nextjs", "framework")],
            &[observation(
                "nextjs",
                "framework",
                RuleObservationStatus::Unknown,
            )],
        );
        assert!(unknown.deactivate().is_empty());

        let migrated = derive_projection(
            &[technology("nextjs", "framework")],
            &[
                observation(
                    "nextjs",
                    "framework",
                    RuleObservationStatus::ConfirmedAbsent,
                ),
                observation("remix", "framework", RuleObservationStatus::Detected),
            ],
        );
        assert_eq!(migrated.changes().len(), 1);
        assert_eq!(migrated.changes()[0].kind(), TechnologyChangeKind::Migrated);
    }
}
