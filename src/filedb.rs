use std::{collections::HashMap, sync::Arc};

use async_lsp::lsp_types::TextDocumentContentChangeEvent;
use parking_lot::RwLock;
use ropey::{LineType, Rope};
use tree_sitter::{Point, Tree};

use crate::utils::{parse_file, position_to_point, reparse_file};

#[derive(Default)]
pub struct FileDatabase {
    pub(crate) files: Arc<RwLock<HashMap<String, SourceFile>>>,
}

impl FileDatabase {
    pub fn file_opened(&self, file_path: &str, file_content: String) {
        let s = file_content.to_string();
        let Some(tree) = parse_file(&s) else {
            return;
        };
        let rope = Rope::from(s);
        self.files.write().insert(
            file_path.to_string(),
            SourceFile {
                content: rope,
                tree,
            },
        );
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

            let new_line_breaks = change.text.chars().filter(|c| c == &'\n').count();
            let mut trailing_characters = 0;
            for ch in change.text.chars().rev() {
                if ch == '\n' {
                    break;
                }
                trailing_characters += 1;
            }

            let new_end_byte = start + change.text.len();
            let new_end_position = Point::new(
                range.start.line as usize + new_line_breaks,
                trailing_characters,
            );

            file.tree.edit(&tree_sitter::InputEdit {
                start_byte: start,
                old_end_byte: end,
                new_end_byte,
                start_position: position_to_point(range.start),
                old_end_position: position_to_point(range.end),
                new_end_position,
            });
        }
        if let Some(new_tree) = reparse_file(&file.content.to_string(), &file.tree) {
            file.tree = new_tree
        }
    }
}

pub struct SourceFile {
    pub(crate) content: Rope,
    pub(crate) tree: Tree,
}
