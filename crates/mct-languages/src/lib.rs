//! The languages this project ships. `mct-cli`, `mct-mcp-server` and
//! `mct-eval` all index through [`build_registry`], so wiring a new
//! `mct-lang-*` crate in is one dependency in this crate's `Cargo.toml` and
//! one `register` line here — no binary can ship a different language set.

use std::sync::Arc;

use mct_core::LanguageRegistry;

pub fn build_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(mct_lang_rust::RustParser));
    registry.register(Arc::new(mct_lang_python::PythonParser));
    registry.register(Arc::new(mct_lang_java::JavaParser));
    registry.register(Arc::new(mct_lang_kotlin::KotlinParser));
    registry.register(Arc::new(mct_lang_csharp::CSharpParser));
    registry.register(Arc::new(mct_lang_js_ts::JsTsParser));
    registry.register(Arc::new(mct_lang_cpp::CppParser));
    registry.register(Arc::new(mct_lang_go::GoParser));
    registry.register(Arc::new(mct_lang_html::HtmlParser));
    registry.register(Arc::new(mct_lang_css::CssParser));
    registry.register(Arc::new(mct_lang_xml::XmlParser));
    registry.register(Arc::new(mct_lang_xaml::XamlParser));
    registry.register(Arc::new(mct_lang_bash::BashParser));
    registry.register(Arc::new(mct_lang_powershell::PowerShellParser));
    registry.register(Arc::new(mct_lang_php::PhpParser));
    registry.register(Arc::new(mct_lang_md::MarkdownParser));
    registry.register(Arc::new(mct_lang_lua::LuaParser));
    registry
}

#[cfg(test)]
mod tests {
    // Test code: reads this repository's own crates/ directory, never
    // repo-input content.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::fs;
    use std::path::Path;

    use super::build_registry;

    #[test]
    fn every_language_crate_in_the_workspace_is_registered() {
        // One `crates/mct-lang-*` crate is one language id (JS/TS included).
        // An implemented but unregistered parser is silently skipped by
        // `init`/`status` and by the MCP server.
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut lang_crates: Vec<String> = fs::read_dir(&crates_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("mct-lang-"))
            .collect();
        lang_crates.sort_unstable();
        assert!(!lang_crates.is_empty());

        let manifest = fs::read_to_string(crates_dir.join("mct-languages/Cargo.toml")).unwrap();
        for name in &lang_crates {
            assert!(
                manifest.contains(&format!("{name}.workspace = true")),
                "mct-languages/Cargo.toml does not depend on {name}"
            );
        }
        assert_eq!(
            build_registry().language_ids().len(),
            lang_crates.len(),
            "registered languages vs {lang_crates:?}"
        );
    }

    #[test]
    fn a_representative_extension_resolves_for_each_language() {
        let registry = build_registry();
        let pairs = [
            ("sh", "bash"),
            ("hpp", "cpp"),
            ("cs", "csharp"),
            ("css", "css"),
            ("go", "go"),
            ("htm", "html"),
            ("java", "java"),
            ("tsx", "javascript_typescript"),
            ("kts", "kotlin"),
            ("lua", "lua"),
            ("md", "markdown"),
            ("php", "php"),
            ("psm1", "powershell"),
            ("pyi", "python"),
            ("rs", "rust"),
            ("xaml", "xaml"),
            ("xml", "xml"),
        ];
        for (extension, language) in pairs {
            let parser = registry
                .for_extension(extension)
                .unwrap_or_else(|| panic!("no parser registered for `.{extension}`"));
            assert_eq!(parser.language_id(), language, ".{extension}");
        }
        // A new language needs its own pair above.
        assert_eq!(registry.language_ids().len(), pairs.len());
    }
}
