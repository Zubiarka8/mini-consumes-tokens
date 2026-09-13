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
    registry.register(Arc::new(ccm_lang_csharp::CSharpParser));
    registry.register(Arc::new(ccm_lang_js_ts::JsTsParser));
    registry.register(Arc::new(ccm_lang_cpp::CppParser));
    registry.register(Arc::new(ccm_lang_go::GoParser));
    registry
}
