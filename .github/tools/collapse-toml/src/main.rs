// Collapses TOML 1.1 multiline inline tables (allowed by toml_edit/Cargo)
// into single-line inline tables, since tauri-action's bundled TOML parser
// is TOML 1.0-only and rejects newlines inside `{ ... }`. Comments and
// surrounding formatting are left untouched; this only runs in CI against
// the checkout, never committed back, so the repo keeps the readable
// multiline style.
use std::{env, fs, process};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Value};

fn collapse_item(item: &mut Item) {
    match item {
        Item::Table(table) => {
            for (_k, v) in table.iter_mut() {
                collapse_item(v);
            }
        }
        Item::Value(Value::InlineTable(table)) => collapse_inline_table(table),
        _ => {}
    }
}

fn collapse_inline_table(table: &mut InlineTable) {
    let mut fresh = InlineTable::new();
    for (k, v) in table.iter() {
        let mut v = v.clone();
        v.decor_mut().clear();
        collapse_nested(&mut v);
        fresh.insert(k, v);
    }
    *table = fresh;
}

fn collapse_nested(value: &mut Value) {
    match value {
        Value::InlineTable(table) => collapse_inline_table(table),
        Value::Array(array) => {
            let mut fresh = Array::new();
            for v in array.iter() {
                let mut v = v.clone();
                v.decor_mut().clear();
                collapse_nested(&mut v);
                fresh.push(v);
            }
            *array = fresh;
        }
        _ => {}
    }
}

fn main() {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: collapse-toml <path>...");
        process::exit(1);
    }

    for path in paths {
        let src = fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("failed to read {path}: {e}");
            process::exit(1);
        });
        let mut doc: DocumentMut = src.parse().unwrap_or_else(|e| {
            eprintln!("failed to parse {path}: {e}");
            process::exit(1);
        });
        for (_k, item) in doc.iter_mut() {
            collapse_item(item);
        }
        fs::write(&path, doc.to_string()).unwrap_or_else(|e| {
            eprintln!("failed to write {path}: {e}");
            process::exit(1);
        });
        println!("collapsed inline tables in {path}");
    }
}
