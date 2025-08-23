use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point};

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
