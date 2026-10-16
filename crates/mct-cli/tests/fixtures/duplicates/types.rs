// Primitive spellings are shadowable; conflicts cannot prove incompatibility.
type u32 = String;
type Alias = String;
fn direct_a(x: u32) -> u32 { let value = x; value }
fn direct_b(x: u64) -> u64 { let value = x; value }
fn direct_c(x: Alias) -> Alias { let value = x; value }
fn compound_a(x: Option<u32>) -> Option<u32> { x }
fn compound_b(x: Option<u64>) -> Option<u64> { x }
