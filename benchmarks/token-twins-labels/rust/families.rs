//! Groups whose members form more than one family, and one group that does not.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

// Case rs-split-sinks: storage writers and block hashers share one loop shape.
fn write_records(writer: &mut Writer, records: &[Record]) -> usize {
    let mut written = 0;
    for record in records {
        if record.deleted {
            continue;
        }
        writer.append(record.key, record.value);
        written += 1;
    }
    written
}

fn write_entries(sink: &mut Writer, entries: &[Record]) -> usize {
    let mut count = 0;
    for entry in entries {
        if entry.deleted {
            continue;
        }
        sink.append(entry.key, entry.value);
        count += 1;
    }
    count
}

fn hash_blocks(hasher: &mut Hasher, blocks: &[Block]) -> usize {
    let mut hashed = 0;
    for block in blocks {
        if block.empty {
            continue;
        }
        hasher.update(block.offset, block.bytes);
        hashed += 1;
    }
    hashed
}

fn hash_chunks(digest: &mut Hasher, chunks: &[Block]) -> usize {
    let mut seen = 0;
    for chunk in chunks {
        if chunk.empty {
            continue;
        }
        digest.update(chunk.offset, chunk.bytes);
        seen += 1;
    }
    seen
}

// Case rs-single-pairs: three renamed copies of one key=value parser.
fn parse_pairs(input: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for part in input.split(';') {
        if let Some((key, value)) = part.split_once('=') {
            pairs.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    pairs
}

fn parse_labels(text: &str) -> Vec<(String, String)> {
    let mut labels = Vec::new();
    for item in text.split(';') {
        if let Some((name, data)) = item.split_once('=') {
            labels.push((name.trim().to_owned(), data.trim().to_owned()));
        }
    }
    labels
}

fn parse_tags(raw: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    for piece in raw.split(';') {
        if let Some((tag, content)) = piece.split_once('=') {
            tags.push((tag.trim().to_owned(), content.trim().to_owned()));
        }
    }
    tags
}
