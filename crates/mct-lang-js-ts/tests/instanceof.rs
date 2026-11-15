//! The right side of `x instanceof Marker` names a class: a use of it, so a
//! class only ever tested that way is not dead code. A call on the right,
//! a member (`ns.Marker`) and a left operand are not this relation.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"class Marker {}
class Other {}
function check(o: unknown, Local: any) {
  if (o instanceof Marker) return 1;
  if (o instanceof ns.Other) return 2;
  if (o instanceof Local) return 3;
  return Other === o;
}
class Holder {
  is(o: unknown): boolean { return o instanceof Third; }
}
"#;

#[test]
fn the_class_on_the_right_of_instanceof_is_referenced() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/a.ts", "TypeScript", SOURCE);
    let refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(refs, [("Marker", 4), ("Other", 5), ("Third", 10)]);
}
