pub fn assert_default_excludes(paths: &[&str]) {
    let set = mct_index::ExcludeSet::default();
    for path in paths {
        assert!(set.is_excluded(path), "expected default exclusion: {path}");
    }
}

pub fn assert_default_includes(paths: &[&str]) {
    let set = mct_index::ExcludeSet::default();
    for path in paths {
        assert!(!set.is_excluded(path), "expected default inclusion: {path}");
    }
}
