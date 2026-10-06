//! Long, multi-file fixture corpus (issue #74): a WPF warehouse client —
//! `App.xaml` (shared resources), `Themes/Controls.xaml` (styles and
//! templates) and four views (`MainWindow`, `OrdersView`, `InventoryView`,
//! `OrderEditorDialog`) — each file 300–600 lines and using the others'
//! resources, styles and templates by key. The shared checks (size, line
//! ranges, golden snapshot, index round trip, malformed input) come from
//! `mct-corpus`; the tests below pin the naming and event-handler rules XAML
//! is expected to follow, and its documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xaml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_xaml::XamlParser;

mct_corpus::standard_tests!(XamlParser);

fn parent<'a>(path: &str, name: &str) -> Option<&'a str> {
    corpus()
        .symbol(path, name, SymbolKind::Element)
        .parent
        .as_deref()
}

fn handler(path: &str, from: &str, to: &str) {
    corpus().relation(path, from, RelationKind::References, to);
}

#[test]
fn every_file_is_a_module_named_after_the_file() {
    for (path, name) in [
        ("App.xaml", "App"),
        ("Themes/Controls.xaml", "Controls"),
        ("Views/MainWindow.xaml", "MainWindow"),
        ("Views/OrdersView.xaml", "OrdersView"),
        ("Views/InventoryView.xaml", "InventoryView"),
        ("Views/OrderEditorDialog.xaml", "OrderEditorDialog"),
    ] {
        let m = corpus().symbol(path, name, SymbolKind::Module);
        assert_eq!((m.location.line, m.parent.as_deref()), (1, None), "{path}");
    }
}

#[test]
fn x_name_and_plain_name_name_an_element() {
    // `x:Name` on the root window and on controls…
    assert_eq!(parent("MainWindow.xaml", "ShellWindow"), None);
    assert_eq!(parent("MainWindow.xaml", "LayoutRoot"), Some("ShellWindow"));
    // …plain `Name` on the editor's root…
    assert_eq!(parent("OrderEditorDialog.xaml", "EditorWindow"), None);
    // …and with both, `x:Name` wins.
    assert_eq!(
        parent("OrdersView.xaml", "OnlyOverdue"),
        Some("OrdersToolbar")
    );
    assert!(corpus().symbols_named("OverdueFilterLegacy").is_empty());
}

#[test]
fn x_key_does_not_name_an_element() {
    // Resources are keyed, not named: no symbol for brushes or styles.
    for key in ["PrimaryBrush", "BaseButton", "CustomerChip", "TrayMenu"] {
        assert!(corpus().symbols_named(key).is_empty(), "{key}");
    }
}

#[test]
fn parent_is_the_nearest_named_ancestor() {
    assert_eq!(parent("MainWindow.xaml", "GlobalSearch"), Some("HeaderBar"));
    assert_eq!(parent("MainWindow.xaml", "OrdersTab"), Some("Navigation"));
    assert_eq!(parent("MainWindow.xaml", "ThumbSync"), Some("Taskbar"));
    assert_eq!(
        parent("OrderEditorDialog.xaml", "Street"),
        Some("CustomerForm")
    );
    assert_eq!(
        parent("OrderEditorDialog.xaml", "CustomerForm"),
        Some("CustomerTab")
    );
    assert_eq!(
        parent("InventoryView.xaml", "CountedQuantity"),
        Some("CycleCountForm")
    );
    // Template parts belong to the part that encloses them, or to nothing
    // when the template sits in an unnamed style.
    assert_eq!(parent("Controls.xaml", "PART_Content"), Some("PART_Border"));
    assert_eq!(parent("Controls.xaml", "PART_Border"), None);
    assert_eq!(parent("App.xaml", "ToastClose"), Some("ToastRoot"));
    assert_eq!(parent("InventoryView.xaml", "ZoneFill"), Some("ZoneBorder"));
}

#[test]
fn event_attributes_reference_their_handler() {
    // On a named element, from that element.
    handler("MainWindow.xaml", "ShellWindow", "ShellWindow_Loaded");
    handler("MainWindow.xaml", "ShellWindow", "ShellWindow_Closing");
    handler(
        "MainWindow.xaml",
        "GlobalSearch",
        "GlobalSearch_TextChanged",
    );
    handler("App.xaml", "TraySync", "TraySync_Click");
    handler("InventoryView.xaml", "InventoryRoot", "InventoryRoot_Drop");
    handler(
        "InventoryView.xaml",
        "PutAwayPanel",
        "PutAwayPanel_DragOver",
    );
    handler(
        "OrderEditorDialog.xaml",
        "EditorWindow",
        "EditorWindow_KeyDown",
    );
    handler("Controls.xaml", "PART_Value", "Stepper_PreviewKeyDown");
    handler("Controls.xaml", "PART_Value", "Stepper_MouseWheel");
    // On an unnamed root, from the file module.
    handler("OrdersView.xaml", "OrdersView", "OrdersView_Loaded");
    handler("App.xaml", "App", "App_Activated");
    // On an unnamed element, from its nearest named ancestor (or the
    // module when there is none).
    handler("MainWindow.xaml", "HeaderBar", "Profile_Click");
    handler("OrdersView.xaml", "OrdersGrid", "OpenOrder_Click");
    handler("App.xaml", "App", "OfflineMode_Checked");
}

#[test]
fn values_that_are_not_handler_names_are_not_references() {
    let c = corpus();
    // `ValueChanged="{Binding …}"` and `TextChanged="1.0"`.
    assert!(!c.has_relation(
        "PickPriority",
        RelationKind::References,
        "{Binding OnPriorityChanged}"
    ));
    assert!(
        c.relations()
            .iter()
            .all(|r| !r.to.contains(['{', ' ', '.'])),
        "only bare identifiers"
    );
    assert!(!c.has_relation("ExternalId", RelationKind::References, "1.0"));
}

#[test]
fn handlers_outside_the_known_event_list_are_the_documented_limit() {
    let c = corpus();
    // Not in the parser's event list: command bindings, DataGrid sorting and
    // editing, double clicks, date pickers…
    for (from, to) in [
        ("ShellWindow", "NewOrder_Executed"),
        ("ShellWindow", "NewOrder_CanExecute"),
        ("OrdersGrid", "OrdersGrid_Sorting"),
        ("OrdersGrid", "OrdersGrid_MouseDoubleClick"),
        ("StockGrid", "StockGrid_CellEditEnding"),
        ("DeliveryDate", "DeliveryDate_SelectedDateChanged"),
    ] {
        assert!(
            !c.has_relation(from, RelationKind::References, to),
            "{from} -> {to}"
        );
    }
    // …and `<EventSetter Handler="…"/>`, whose handler is not in an event
    // attribute at all.
    assert!(c.symbols_named("TextBox_SelectAllOnFocus").is_empty());
    assert!(!c
        .relations()
        .iter()
        .any(|r| r.to == "TextBox_SelectAllOnFocus"));
}

#[test]
fn only_references_are_emitted_and_none_cross_files() {
    let c = corpus();
    assert!(c
        .relations()
        .iter()
        .all(|r| r.kind == RelationKind::References));
    // Handlers live in code-behind (`*.xaml.cs`), never in another XAML file.
    assert_eq!(c.cross_file_relation_count(), 0);
}

#[test]
fn a_named_element_spans_its_descendants_handlers() {
    // A handler on an unnamed descendant is attributed to the nearest named
    // ancestor; that element spans through its end tag, so the handler lies
    // inside it.
    for f in &corpus().files {
        for r in &f.parsed.relations {
            let owner = f.parsed.symbols.iter().find(|s| s.id == r.from).unwrap();
            let end = owner.location.end_line.unwrap();
            assert!(
                (owner.location.line..=end).contains(&r.location.line),
                "{}:{} {} outside {} {}-{end}",
                f.path,
                r.location.line,
                r.to_name,
                owner.name,
                owner.location.line
            );
        }
    }
}

#[test]
fn index_answers_handler_queries() {
    let index = corpus().index();
    let refs = index.find_references("StockFilter_Checked").unwrap();
    let from: Vec<_> = refs.iter().map(|r| r.from_symbol.as_str()).collect();
    assert_eq!(from, ["AllStock", "LowStock", "ExpiringStock"]);
    let refs = index.find_references("PaymentMethod_Checked").unwrap();
    assert_eq!(refs.len(), 3);
    assert!(refs
        .iter()
        .all(|r| r.relative_path == "Views/OrderEditorDialog.xaml"));
}
