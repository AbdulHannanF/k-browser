use kitsune_agent::profile::{ProfileIndexer, ProfileSummary};
use std::path::PathBuf;

#[test]
fn profile_summary_default_is_empty() {
    let s = ProfileSummary::default();
    assert!(s.full_name.is_empty());
    assert!(s.education.is_empty());
    assert!(s.languages.is_empty());
}

#[test]
fn profile_indexer_new_accepts_path() {
    let indexer = ProfileIndexer::new(PathBuf::from("tests/fixtures/profile"));
    let _ = indexer;
}
