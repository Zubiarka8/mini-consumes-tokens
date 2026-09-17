use std::sync::Arc;

use ccm_core::LanguageRegistry;

/// The set of languages this build of the MCP server knows how to parse.
/// This is the *only* place the binary wires a language crate in — adding
/// `ccm-lang-go` later means one more `registry.register(...)` line here,
/// nothing else in this crate changes.
pub fn build_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(ccm_lang_rust::RustParser));
    registry.register(Arc::new(ccm_lang_python::PythonParser));
    registry.register(Arc::new(ccm_lang_java::JavaParser));
    registry.register(Arc::new(ccm_lang_kotlin::KotlinParser));
    registry.register(Arc::new(ccm_lang_csharp::CSharpParser));
    registry.register(Arc::new(ccm_lang_js_ts::JsTsParser));
    registry.register(Arc::new(ccm_lang_cpp::CppParser));
    registry.register(Arc::new(ccm_lang_go::GoParser));
    registry.register(Arc::new(ccm_lang_html::HtmlParser));
    registry.register(Arc::new(ccm_lang_css::CssParser));
    registry.register(Arc::new(ccm_lang_xml::XmlParser));
    registry.register(Arc::new(ccm_lang_xaml::XamlParser));
    registry.register(Arc::new(ccm_lang_bash::BashParser));
    registry.register(Arc::new(ccm_lang_powershell::PowerShellParser));
    registry.register(Arc::new(ccm_lang_php::PhpParser));
    registry.register(Arc::new(ccm_lang_md::MarkdownParser));
    registry
}
