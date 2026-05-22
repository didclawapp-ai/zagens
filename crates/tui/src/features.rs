//! Feature flags — re-exported from `deepseek-core` (P2 PR4).

pub use deepseek_core::features::{
    Feature, FeatureSpec, Features, FeaturesToml, Stage, FEATURES, feature_from_key,
    feature_spec_by_key, is_known_feature_key, render_feature_table,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn apply_map_toggles_known_features_and_ignores_unknown_keys() {
        let mut features = Features::with_defaults();
        let entries = BTreeMap::from([
            ("mcp".to_string(), false),
            ("shell_tool".to_string(), false),
            ("not_real".to_string(), false),
        ]);

        features.apply_map(&entries);

        assert!(!features.enabled(Feature::Mcp));
        assert!(!features.enabled(Feature::ShellTool));
        assert_eq!(feature_from_key("not_real"), None);
    }

    #[test]
    fn render_feature_table_uses_registry_order_and_effective_state() {
        let mut features = Features::with_defaults();
        features.disable(Feature::Mcp);

        let table = render_feature_table(&features);
        let lines = table.lines().collect::<Vec<_>>();

        assert_eq!(lines.first(), Some(&"feature\tstage\tenabled"));
        assert!(lines.contains(&"shell_tool\tstable\ttrue"));
        assert!(lines.contains(&"mcp\texperimental\tfalse"));
    }
}
