// Subgroup corpus for Rust: one structural group mixing two families of
// fields and callees, plus a pair that differs only by renaming. Parsed,
// never compiled.

fn user_title(record: &Record) -> String {
    let value = record.user_id;
    let found = load_user(value);
    found.display_name.clone()
}

fn order_title(entry: &Record) -> String {
    let key = entry.order_id;
    let found = load_order(key);
    found.total.clone()
}

fn owner_title(row: &Record) -> String {
    let ident = row.user_id;
    let person = load_user(ident);
    person.display_name.clone()
}

fn countdown(steps: u32) -> u32 {
    let next = steps - 1;
    let rest = countdown(next);
    rest + 1
}

fn unwind(depth: u32) -> u32 {
    let lower = depth - 1;
    let below = unwind(lower);
    below + 1
}
