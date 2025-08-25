use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use ropey::{LineType, Rope};
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;
use tree_sitter::Tree;

#[derive(Default)]
pub struct FileDatabase {
    files: Arc<RwLock<HashMap<String, SourceFile>>>,
}

impl FileDatabase {
    pub fn file_opened(&self, file_path: &str, file_content: String) {
        let s = file_content.to_string();
        let rope = Rope::from(s);
        self.files
            .write()
            .insert(file_path.to_string(), SourceFile { content: rope });
    }

    pub fn file_changed(
        &self,
        file_path: &str,
        content_changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        let mut lock = self.files.write();
        let Some(file) = lock.get_mut(file_path) else {
            return;
        };
        for change in content_changes {
            let Some(range) = change.range else {
                continue;
            };
            let start = file
                .content
                .line_to_byte_idx(range.start.line as usize, LineType::LF_CR)
                + range.start.character as usize;
            let end = file
                .content
                .line_to_byte_idx(range.end.line as usize, LineType::LF_CR)
                + range.end.character as usize;
            file.content.remove(start..end);
            if !change.text.is_empty() {
                file.content.insert(start, &change.text);
            }
        }
    }
}

struct SourceFile {
    content: Rope,
}
