use tower_lsp::lsp_types::*;

use crate::{
    symbol_table::SymbolTable,
    typedb::TypeDatabase,
    utils::{parse_file, range_contains},
};

pub fn make_inlay_hints(params: InlayHintParams, typedb: &TypeDatabase) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let range = params.range;
    let path = params.text_document.uri.path();
    let file = std::fs::read_to_string(path).unwrap();
    let tree = parse_file(&file).unwrap();

    let mut st = SymbolTable::new(typedb);
    st.build_table(&tree, &file);

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
