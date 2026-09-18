use std::sync::Arc;

use mct_core::LanguageRegistry;

/// The set of languages this build of the MCP server knows how to parse.
/// This is the *only* place the binary wires a language crate in — adding
/// `mct-lang-go` later means one more `registry.register(...)` line here,
/// nothing else in this crate changes.
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
    registry
}
