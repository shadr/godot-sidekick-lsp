use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point, Tree};

pub const fn position_to_point(position: Position) -> Point {
    Point::new(position.line as usize, position.character as usize)
}

pub const fn point_to_position(point: Point) -> Position {
    Position {
        line: point.row as u32,
        character: point.column as u32,
    }
}

pub fn node_to_range(node: &Node) -> Range {
    Range::new(
        point_to_position(node.start_position()),
        point_to_position(node.end_position()),
    )
}

pub fn parse_file(content: &str) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .unwrap();

    parser.parse(content, None)
}

pub fn node_content<'s>(node: &Node, file_content: &'s str) -> &'s str {
    &file_content[node.start_byte()..node.end_byte()]
}

pub fn range_contains(range: Range, position: Position) -> bool {
    (range.start.line < position.line && range.end.line > position.line)
        || (range.start.line == position.line && range.start.character <= position.character)
        || (range.end.line == position.line && range.end.character >= position.character)
}
