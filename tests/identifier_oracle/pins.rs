//! Defects the oracle found, filed as plan nodes and pinned here until they
//! are fixed.
//!
//! A pin names the site, the corpus entries that fail there today, and the
//! node that fixes them. The main property skips pinned pairs; a separate test
//! asserts each still fails and prints the failure, so the defect is reported
//! on every run and the pin cannot outlive its fix.

/// One pinned defect.
pub struct Pin {
    /// The plan node that fixes it.
    pub node: &'static str,
    pub site: &'static str,
    /// Corpus labels that fail at `site` today.
    pub labels: &'static [&'static str],
}

/// The pins. None are open: every defect the oracle has found is fixed, and
/// its pairs are held by the main property.
pub const PINS: &[Pin] = &[];

/// The pin covering `site` × `label`, if any.
pub fn pinned(site: &str, label: &str) -> Option<&'static Pin> {
    PINS.iter()
        .find(|pin| pin.site == site && pin.labels.contains(&label))
}
