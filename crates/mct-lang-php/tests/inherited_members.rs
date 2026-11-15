//! `$this->m()` is qualified by the enclosing class and pinned to its
//! namespace. A method the class inherits from a trait or a parent class
//! declared in another namespace is still that call's target (ADR-037): the
//! namespace constrains the class named by the qualifier, not the ancestor.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_php::PhpParser;

const SUPPORT: &str = "<?php
namespace Shop\\Support;

trait Loggable
{
    public function log(string $m): void {}
}

abstract class Base
{
    public function save(): void {}
}
";

const DOMAIN: &str = "<?php
namespace Shop\\Domain;

use Shop\\Support\\Base;
use Shop\\Support\\Loggable;

class Order extends Base
{
    use Loggable;

    public function place(): void
    {
        $this->log('placed');
        $this->save();
        $this->missing();
    }
}
";

/// A same-named class in a third namespace that declares neither method.
const OTHER: &str = "<?php
namespace Shop\\Other;

class Order
{
    public function ship(): void {}
}
";

#[test]
fn this_reaches_a_method_inherited_from_another_namespace() {
    let root = std::env::temp_dir().join(format!("mct-php-inherit-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Support.php"), SUPPORT).unwrap();
    std::fs::write(root.join("src/Domain.php"), DOMAIN).unwrap();
    std::fs::write(root.join("src/Other.php"), OTHER).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let mut hits: Vec<String> = index
        .find_calls("place")
        .unwrap()
        .into_iter()
        .map(|h| {
            let target = h.target_id.map(|id| {
                let s = index.symbols_by_ids(&[id]).unwrap().remove(0);
                format!("{}:{}", s.relative_path, s.line)
            });
            format!("{} {:?} {:?}", h.to_name, h.resolution, target)
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "log Resolved Some(\"src/Support.php:6\")",
            "missing Unresolved None",
            "save Resolved Some(\"src/Support.php:11\")",
        ]
    );
}
