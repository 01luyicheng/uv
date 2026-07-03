use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, RangeBounds};

use pubgrub::{Ranges, SetRelation, VersionSet};

use uv_pep440::{Operator, Version, VersionSpecifiers};

/// A PubGrub version range with a bounded pre-release opt-in region.
///
/// `versions` is the logical set used by PubGrub. `prerelease_region` is candidate-selection
/// metadata: a pre-release inside this region participates in normal version ordering, while one
/// outside it is considered only after stable candidates are exhausted. The region is always
/// clipped to `versions`.
#[derive(Clone, Debug)]
pub struct Range<T> {
    versions: Ranges<T>,
    prerelease_region: Option<Box<Ranges<T>>>,
}

impl<T> Deref for Range<T> {
    type Target = Ranges<T>;

    fn deref(&self) -> &Self::Target {
        &self.versions
    }
}

// PubGrub defines equality and hashing in terms of version membership. The admission region affects
// candidate ordering, not which versions satisfy a term, so selection caches compare it separately.
impl<T: PartialEq> PartialEq for Range<T> {
    fn eq(&self, other: &Self) -> bool {
        self.versions == other.versions
    }
}

impl<T: Eq> Eq for Range<T> {}

impl<T: Hash> Hash for Range<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.versions.hash(state);
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> Range<T> {
    fn from_parts(versions: Ranges<T>, prerelease_region: Option<Ranges<T>>) -> Self {
        let prerelease_region = prerelease_region
            .map(|prereleases| prereleases.intersection(&versions))
            .filter(|prereleases| !prereleases.is_empty())
            .map(Box::new);
        Self {
            versions,
            prerelease_region,
        }
    }

    /// Create a range with no pre-release opt-in.
    pub(crate) fn from_versions(versions: Ranges<T>) -> Self {
        Self::from_parts(versions, None)
    }

    pub(crate) fn empty() -> Self {
        Self::from_versions(Ranges::empty())
    }

    pub(crate) fn full() -> Self {
        Self::from_versions(Ranges::full())
    }

    pub(crate) fn singleton(version: T) -> Self {
        Self::from_versions(Ranges::singleton(version))
    }

    pub(crate) fn from_range_bounds(range: impl RangeBounds<T>) -> Self {
        Self::from_versions(Ranges::from_range_bounds(range))
    }

    pub(crate) fn strictly_lower_than(version: T) -> Self {
        Self::from_versions(Ranges::strictly_lower_than(version))
    }

    pub(crate) fn strictly_higher_than(version: T) -> Self {
        Self::from_versions(Ranges::strictly_higher_than(version))
    }

    /// Return the logical version bounds.
    pub(crate) fn versions(&self) -> &Ranges<T> {
        &self.versions
    }

    /// Return the bounded region in which pre-releases are explicitly enabled.
    pub(crate) fn prerelease_region(&self) -> Option<&Ranges<T>> {
        self.prerelease_region.as_deref()
    }

    /// Return whether two ranges have the same bounds and candidate-selection behavior.
    pub(crate) fn selection_eq(&self, other: &Self) -> bool {
        self.versions == other.versions && self.prerelease_region == other.prerelease_region
    }

    pub(crate) fn complement(&self) -> Self {
        // A complement is an exclusion, so it does not grant pre-release admission.
        Self::from_versions(self.versions.complement())
    }

    pub(crate) fn intersection(&self, other: &Self) -> Self {
        Self::from_parts(
            self.versions.intersection(&other.versions),
            combine_regions(
                self.prerelease_region.as_deref(),
                other.prerelease_region.as_deref(),
            ),
        )
    }

    pub(crate) fn union(&self, other: &Self) -> Self {
        Self::from_parts(
            self.versions.union(&other.versions),
            combine_regions(
                self.prerelease_region.as_deref(),
                other.prerelease_region.as_deref(),
            ),
        )
    }

    #[cfg(test)]
    fn difference(&self, other: &Self) -> Self {
        Self::from_parts(
            self.versions.intersection(&other.versions.complement()),
            self.prerelease_region.as_deref().cloned(),
        )
    }
}

fn combine_regions<T: Clone + Ord>(
    left: Option<&Ranges<T>>,
    right: Option<&Ranges<T>>,
) -> Option<Ranges<T>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(right)),
        (Some(region), None) | (None, Some(region)) => Some(region.clone()),
        (None, None) => None,
    }
}

impl From<Ranges<Version>> for Range<Version> {
    fn from(versions: Ranges<Version>) -> Self {
        Self::from_versions(versions)
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> FromIterator<(Bound<T>, Bound<T>)> for Range<T> {
    fn from_iter<I: IntoIterator<Item = (Bound<T>, Bound<T>)>>(iter: I) -> Self {
        Self::from_versions(iter.into_iter().collect())
    }
}

impl From<VersionSpecifiers> for Range<Version> {
    fn from(specifiers: VersionSpecifiers) -> Self {
        let prerelease_region = specifiers
            .iter()
            .filter(|specifier| {
                !matches!(
                    specifier.operator(),
                    Operator::NotEqual | Operator::NotEqualStar
                ) && specifier.any_prerelease()
            })
            .map(|specifier| Ranges::from(specifier.clone()))
            .reduce(|left, right| left.union(&right));
        let versions = Ranges::from(specifiers);
        Self::from_parts(versions, prerelease_region)
    }
}

impl<T: Debug + Display + Clone + Eq + Ord + Hash> VersionSet for Range<T> {
    type V = T;

    fn empty() -> Self {
        Self::empty()
    }

    fn singleton(version: Self::V) -> Self {
        Self::singleton(version)
    }

    fn complement(&self) -> Self {
        Self::complement(self)
    }

    fn intersection(&self, other: &Self) -> Self {
        Self::intersection(self, other)
    }

    fn contains(&self, version: &Self::V) -> bool {
        self.versions.contains(version)
    }

    fn full() -> Self {
        Self::full()
    }

    fn union(&self, other: &Self) -> Self {
        Self::union(self, other)
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        self.versions.is_disjoint(&other.versions)
    }

    fn subset_of(&self, other: &Self) -> bool {
        self.versions.subset_of(&other.versions)
    }

    fn relation(&self, other: &Self) -> SetRelation {
        self.versions.relation(&other.versions)
    }
}

impl<T: Debug + Display + Clone + Eq + Ord> Display for Range<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.versions, formatter)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pubgrub::{SetRelation, VersionSet};

    use super::Range;
    use uv_pep440::{Version, VersionSpecifiers};

    fn range(specifiers: &str) -> Range<Version> {
        Range::from(
            VersionSpecifiers::from_str(specifiers).expect("valid version specifiers for test"),
        )
    }

    #[test]
    fn prerelease_region_is_clipped_to_its_specifier_bounds() {
        let range = range(">=2.0b1,<3").union(&range(">=3.5,<4"));

        assert!(range.contains(&Version::from_str("3.6b1").expect("valid version")));
        assert!(!range.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("3.6b1").expect("valid version"))
        }));
        assert!(range.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("2.5b1").expect("valid version"))
        }));
    }

    #[test]
    fn union_keeps_prerelease_admission_with_its_originating_range() {
        let range = range(">=1.0").union(&range(">=2.0b1"));

        let prereleases = range
            .prerelease_region()
            .expect("the pre-release specifier should create an opt-in region");
        assert!(!prereleases.contains(&Version::from_str("1.5b1").expect("valid version")));
        assert!(prereleases.contains(&Version::from_str("2.0b1").expect("valid version")));
    }

    #[test]
    fn narrowing_permanently_sheds_prerelease_admission() {
        let narrowed = range(">=2.0b1").intersection(&range("<3"));
        let widened = narrowed.union(&range(">=3.5,<4"));

        assert!(widened.contains(&Version::from_str("3.6b1").expect("valid version")));
        assert!(!widened.prerelease_region().is_some_and(|region| {
            region.contains(&Version::from_str("3.6b1").expect("valid version"))
        }));
    }

    #[test]
    fn difference_does_not_inherit_prerelease_admission() {
        let requirement = range(">=1.0");
        let exclusion = range(">=2.0b1");
        let range = requirement.difference(&exclusion);
        let via_complement = requirement.intersection(&exclusion.complement());

        assert_eq!(range, via_complement);
        assert!(range.prerelease_region().is_none());
        assert!(via_complement.prerelease_region().is_none());
        assert!(range.contains(&Version::from_str("1.5a1").expect("valid version")));
    }

    #[test]
    fn complement_erases_prerelease_admission() {
        let range = range(">=2.0b1");
        let complemented = range.complement().complement();

        assert_eq!(range.versions(), complemented.versions());
        assert!(complemented.prerelease_region().is_none());
    }

    #[test]
    fn set_relations_ignore_prerelease_admission() {
        let plain = range(">=1.0");
        let opted_in = plain.intersection(&range(">=1.0a1"));

        assert_eq!(plain, opted_in);
        assert!(!plain.selection_eq(&opted_in));
        assert_eq!(plain.relation(&opted_in), SetRelation::Subset);
        assert!(plain.subset_of(&opted_in));
        assert!(!plain.is_disjoint(&opted_in));
    }
}
