#[derive(Debug, Clone, Default)]
pub struct PluginOptions {
    /// List of identifiers that will not be captured in worklet closures.
    pub globals: Vec<String>,

    /// If true, no global identifiers are implicitly captured in worklet closures.
    pub strict_global: bool,

    /// If true, omits native-only data (init_data) from the output.
    pub omit_native_only_data: bool,

    /// If true, disables source map generation for worklets.
    pub disable_source_maps: bool,

    /// If true, uses relative file paths for source locations.
    pub relative_source_location: bool,

    /// If true, disables worklet class support.
    pub disable_worklet_classes: bool,

    /// The filename of the file being transformed.
    pub filename: Option<String>,

    /// The current working directory.
    pub cwd: Option<String>,

    /// If true, this is a release build (skips debug info like stack details, version, location).
    pub is_release: bool,

    /// The version string to embed as `__pluginVersion`.
    /// Injected from the JS side (e.g. the installed react-native-worklets package version).
    pub plugin_version: String,
}
