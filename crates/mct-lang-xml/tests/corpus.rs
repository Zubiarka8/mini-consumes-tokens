//! Long, multi-file fixture corpus (issue #74): the configuration of a
//! warehouse service — Spring application, datasource and security contexts,
//! a Logback configuration, a BPMN order workflow and a product catalog —
//! each file 300–600 lines and naming beans, loggers and flow nodes the
//! others point at (`ref="…"`, `<import resource>`, `sourceRef`,
//! `categoryRef`). The shared checks (size, line ranges, golden snapshot,
//! index round trip, malformed input) come from `mct-corpus`; the tests below
//! pin the naming conventions XML is expected to follow and its documented
//! limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_xml::XmlParser;

mct_corpus::standard_tests!(XmlParser);

fn element<'a>(path: &str, name: &str) -> &'a mct_core::SymbolRecord {
    corpus().symbol(path, name, SymbolKind::Element)
}

fn parent<'a>(path: &str, name: &str) -> Option<&'a str> {
    element(path, name).parent.as_deref()
}

#[test]
fn every_file_is_a_module_named_after_the_file() {
    for (path, name) in [
        ("config/application.xml", "application"),
        ("config/datasource.xml", "datasource"),
        ("config/security.xml", "security"),
        ("config/logging.xml", "logging"),
        ("workflow/orders.xml", "orders"),
        ("catalog/products.xml", "products"),
    ] {
        let module = corpus().symbol(path, name, SymbolKind::Module);
        assert_eq!(module.location.line, 1, "{path}");
        assert_eq!(module.parent, None, "{path}");
    }
}

#[test]
fn id_name_and_capitalized_name_each_name_an_element() {
    // `id` (beans, flow nodes, products), `name` (loggers, appenders,
    // catalog attributes) and `Name` (ERP labels).
    element("application.xml", "orderService");
    element("logging.xml", "sqlLogger");
    element("logging.xml", "ASYNC_JSON");
    element("products.xml", "storage");
    element("products.xml", "Cocina");
    // With both, `id` wins and the `name` is not a second symbol.
    element("orders.xml", "msgPaymentConfirmed");
    assert!(corpus().symbols_named("PaymentConfirmed").is_empty());
    element("orders.xml", "approveLargeOrder");
    assert!(corpus().symbols_named("Approve large order").is_empty());
}

#[test]
fn unnamed_and_empty_named_elements_are_skipped() {
    // `<category id="">` is anonymous: its child has no named parent.
    assert_eq!(parent("products.xml", "cat-clearance"), None);
    // A variant without an id leaves no symbol, its siblings with one do.
    assert_eq!(parent("products.xml", "sku-1201-wht"), Some("sku-1201"));
    let variants = corpus()
        .symbols_named("sku-1201-red")
        .into_iter()
        .chain(corpus().symbols_named("sku-1201-wht"))
        .count();
    assert_eq!(variants, 2);
}

#[test]
fn parent_is_the_nearest_named_ancestor() {
    // Nested categories, skipping the unnamed `<categories>` wrapper.
    assert_eq!(parent("products.xml", "cat-home"), None);
    assert_eq!(parent("products.xml", "cat-kitchen"), Some("cat-home"));
    assert_eq!(parent("products.xml", "cat-cookware"), Some("cat-kitchen"));
    assert_eq!(parent("products.xml", "cat-dairy"), Some("cat-chilled"));
    // Through unnamed `<constructor-arg>`/`<list>`/`<bean>` wrappers…
    assert_eq!(parent("application.xml", "endpoint"), Some("exchangeRates"));
    // …but a named `<property name="caches">` is itself the owner.
    assert_eq!(parent("application.xml", "productCache"), Some("caches"));
    assert_eq!(
        parent("application.xml", "catalogImporter"),
        Some("importer")
    );
    // BPMN: sub-process nodes belong to the sub-process.
    assert_eq!(
        parent("orders.xml", "pickChilled"),
        Some("pickingSubprocess")
    );
    assert_eq!(
        parent("orders.xml", "pickingSubprocess"),
        Some("orderFulfilment")
    );
    assert_eq!(
        parent("orders.xml", "orderFulfilment"),
        Some("warehouseDefinitions")
    );
    // Prefixed elements (`beans:bean`, `tx:method`) behave the same.
    assert_eq!(parent("security.xml", "passwordPolicy"), Some("policy"));
    assert_eq!(parent("datasource.xml", "reserve*"), Some("txAdvice"));
}

#[test]
fn an_element_spans_its_start_tag_only() {
    // `<bean id="exchangeRates" …` wraps onto a second line; the element
    // ends with its start tag, not with `</bean>`.
    let rates = element("application.xml", "exchangeRates");
    assert_eq!(
        (rates.location.line, rates.location.end_line),
        (99, Some(100))
    );
}

#[test]
fn unicode_names_are_kept_verbatim() {
    element("products.xml", "sup-kōbō");
    element("products.xml", "工房たなか");
    element("products.xml", "Lácteos");
    element("security.xml", "müller@example.de");
    element("logging.xml", "com.example.warehouse.i18n.Mensajería");
}

#[test]
fn a_name_repeated_under_another_owner_is_another_symbol() {
    // The `test` profile redefines `auditLogger` inside `<springProfile>`.
    let owners: Vec<_> = corpus()
        .symbols_named("auditLogger")
        .into_iter()
        .map(|(_, s)| s.parent.as_deref())
        .collect();
    assert_eq!(owners, [None, Some("test")]);
}

#[test]
fn same_name_in_several_files_stays_one_symbol_per_file() {
    // `dataSource` is a bean in datasource.xml and a `<property name=…>`
    // under other beans elsewhere.
    let files: Vec<_> = corpus()
        .symbols_named("dataSource")
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(files.contains(&"config/datasource.xml"));
    assert!(files.len() > 3, "{files:?}");
}

#[test]
fn references_between_files_are_the_documented_limit() {
    // `ref="clock"`, `<import resource>`, `sourceRef`/`targetRef` and
    // `categoryRef` all point at elements of other files, but XML emits no
    // relation of any kind.
    let c = corpus();
    assert!(c.relations().is_empty());
    assert_eq!(c.cross_file_relation_count(), 0);
}

#[test]
fn index_finds_elements_across_files() {
    let index = corpus().index();
    let hits = index.find_symbol("auditLogger").unwrap();
    assert!(hits
        .iter()
        .any(|h| h.relative_path == "config/logging.xml" && h.kind == "element"));
    let hits = index.find_symbol("txManager").unwrap();
    assert!(hits
        .iter()
        .any(|h| h.relative_path == "config/datasource.xml"));
}
