use tower_lsp::lsp_types::*;

use crate::{
    symbol_table::{SymbolTable, SymbolType},
    utils::{parse_file, range_contains},
};

pub fn make_inlay_hints(params: InlayHintParams) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let range = params.range;
    let path = params.text_document.uri.path();
    let file = std::fs::read_to_string(&path).unwrap();
    let tree = parse_file(&file).unwrap();

    let mut st = SymbolTable::default();
    st.build_table(&tree, &file);

    for scope in st.map.values() {
        for symbol in &scope.vars {
            if !range_contains(range, symbol.hint_position) {
                continue;
            }
            if symbol.ttype == SymbolType::None || symbol.ttype == SymbolType::Unknown {
                continue;
            }
            if symbol.static_typed {
                continue;
            }

            hints.push(InlayHint {
                position: symbol.hint_position,
                label: InlayHintLabel::String(format!(": {:?}", symbol.ttype)),
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
