// Namespace items are not bound by an equally spelled parameter.
fn fetch_first(input: i64) -> i64 {
    if input < 0 {
        return 0;
    }
    if input == 0 {
        return 1;
    }
    helpers::input(input)
}

fn fetch_second(data: i64) -> i64 {
    if data < 0 {
        return 0;
    }
    if data == 0 {
        return 1;
    }
    helpers::data(data)
}
