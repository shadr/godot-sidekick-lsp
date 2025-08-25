use tower_lsp::lsp_types::*;

use crate::{
    filedb::FileDatabase, symbol_table::SymbolTable, typedb::TypeDatabase, utils::range_contains,
};

pub fn make_inlay_hints(
    range: Range,
    path: &str,
    typedb: &TypeDatabase,
    filedb: &FileDatabase,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let lock = filedb.files.read();
    let Some(source_file) = lock.get(path) else {
        return hints;
    };
    let file = source_file.content.to_string();
    let tree = &source_file.tree;

    let mut st = SymbolTable::new(typedb);
    st.build_table(tree, &file);

    for scope in st.map.values() {
        for symbol in &scope.vars {
            if !range_contains(range, symbol.hint_position) {
                continue;
            }
            let Some(ttype) = &symbol.ttype else {
                continue;
            };
            // if ttype == &SymbolType::Variant(VariantType::Nil) {
            //     continue;
            // }
            if symbol.static_typed {
                continue;
            }

            hints.push(InlayHint {
                position: symbol.hint_position,
                label: InlayHintLabel::String(format!(": {}", ttype.to_string())),
                kind: None,
                text_edits: None,
                tooltip: None,
                padding_left: None,
                padding_right: None,
                data: None,
            });
        }
    }

    hints
}
