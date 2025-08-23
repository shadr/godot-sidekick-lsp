use std::collections::HashSet;

use streaming_iterator::StreamingIterator;
use tower_lsp::lsp_types::*;
use tree_sitter::{Language, Node, Query, QueryCursor, Tree};

use crate::utils::{node_content, parse_file, point_to_position, position_to_point};

pub fn extract_into_function_action(params: &CodeActionParams) -> Option<CodeActionOrCommand> {
    let file_path = params.text_document.uri.path();
    let range = params.range;
    if range.start == range.end {
        return None;
    }

    let file_content = std::fs::read_to_string(file_path).unwrap();
    let tree = parse_file(&file_content).unwrap();

    let Some((start_node, end_node)) =
        start_end_nodes_from_range(tree.root_node(), range, &file_content)
    else {
        return None;
    };

    let mut start_byte = start_node.start_byte();
    while start_byte > 0 && file_content.as_bytes()[start_byte - 1] != b'\n' {
        start_byte -= 1;
    }
    let content = &file_content[start_byte..end_node.end_byte()];

    let insert_pos = find_insert_position(start_node);

    let new_arguments = collect_non_declared_variables(&tree, start_node, end_node, &file_content);
    let new_arguments = new_arguments.into_iter().collect::<Vec<_>>().join(", ");

    let previous_text_replacement = if end_node.kind() == "variable_statement" {
        let name_node = end_node.child_by_field_name("name").unwrap();
        let name = node_content(&name_node, &file_content);
        format!("var {name} = fun_name({new_arguments})")
    } else {
        format!("fun_name({})", new_arguments)
    };

    let previous_indent_size = calculate_previous_indent_size(content);
    let previous_indent_str = format!("\n{}", "\t".repeat(previous_indent_size));

    let mut insert_text = format!("\n\n\nfunc fun_name({}):\n{}", new_arguments, content);
    if end_node.kind() == "variable_statement" {
        let name_node = end_node.child_by_field_name("name").unwrap();
        let name = node_content(&name_node, &file_content);
        insert_text += &format!("\n\treturn {name}");
    }
    insert_text = insert_text.replace(&previous_indent_str, "\n\t");

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Extract into function".to_string(),
        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: None,
            document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: tower_lsp::lsp_types::Url::from_file_path(file_path).unwrap(),
                    version: None,
                },
                edits: vec![
                    OneOf::Left(TextEdit::new(
                        Range::new(
                            point_to_position(start_node.start_position()),
                            point_to_position(end_node.end_position()),
                        ),
                        previous_text_replacement,
                    )),
                    OneOf::Left(TextEdit::new(
                        Range::new(point_to_position(insert_pos), point_to_position(insert_pos)),
                        insert_text,
                    )),
                ],
            }])),
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }))
}

fn walk_from_start_to_end_node(start_node: Node, end_node: Node, mut callback: impl FnMut(Node)) {
    let mut cur_node_option = Some(start_node);
    while let Some(cur_node) = cur_node_option {
        callback(cur_node);
        if cur_node == end_node {
            break;
        } else {
            cur_node_option = cur_node.next_sibling();
        }
    }
}

fn collect_non_declared_variables(
    tree: &Tree,
    start_node: Node,
    end_node: Node,
    file: &str,
) -> HashSet<String> {
    let top_level_variables = collect_top_level_variable_definitions(tree, file);
    let declared_variables = collect_variable_definitions(start_node, end_node, file);

    let mut used_variables = HashSet::new();
    walk_from_start_to_end_node(start_node, end_node, |node| {
        used_variables.extend(collect_used_variables(node, file));
    });

    used_variables
        .retain(|var| !declared_variables.contains(var) && !top_level_variables.contains(var));

    used_variables
}

fn collect_used_variables(node: Node, file: &str) -> HashSet<String> {
    let query = "(binary_operator (identifier) @used)
(arguments (identifier) @used)
(attribute . (identifier) @used)";
    let query = Query::new(&Language::new(tree_sitter_gdscript::LANGUAGE), query).unwrap();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(&query, node, file.as_bytes());
    let mut names = HashSet::new();
    while let Some(capture) = captures.next() {
        for capture in capture.0.captures {
            let identifier_name = node_content(&capture.node, file);
            names.insert(identifier_name.to_string());
        }
    }
    names
}

fn collect_top_level_variable_definitions(tree: &Tree, file: &str) -> Vec<String> {
    let start_node = tree.root_node().child(0).unwrap();
    let end_node = tree
        .root_node()
        .child(tree.root_node().child_count() - 1)
        .unwrap();
    collect_variable_definitions(start_node, end_node, file)
}

fn collect_variable_definitions(start_node: Node, end_node: Node, file: &str) -> Vec<String> {
    let mut variables = Vec::new();

    walk_from_start_to_end_node(start_node, end_node, |node| {
        if node.kind() == "variable_statement" {
            let name_node = node.child_by_field_name("name").unwrap();
            let name = node_content(&name_node, file);
            variables.push(name.to_string());
        }
    });

    variables
}

fn calculate_previous_indent_size(content: &str) -> usize {
    let mut previous_indent_size = 0;
    for c in content.chars() {
        if c != '\t' {
            break;
        }
        previous_indent_size += 1;
    }
    previous_indent_size
}

fn find_insert_position(start_node: Node<'_>) -> tree_sitter::Point {
    let mut parent = start_node.parent().unwrap();
    while parent.kind() != "function_definition" {
        parent = parent.parent().unwrap()
    }
    let insert_pos = parent.end_position();
    insert_pos
}

fn get_first_non_whitespace_position(mut position: Position, file: &str) -> Position {
    let mut line = file;
    let mut passed_new_lines = 0;
    for (i, c) in file.chars().enumerate() {
        if c == '\n' {
            passed_new_lines += 1;
        }
        if passed_new_lines == position.line {
            line = &file[i + 1..];
            break;
        }
    }
    for (i, c) in line.chars().enumerate() {
        if c != '\t' {
            position.character = i as u32;
            return position;
        }
    }
    return position;
}

fn node_from_position<'a, 'b>(
    root_node: Node<'b>,
    mut position: Position,
    file: &'a str,
) -> Option<Node<'b>> {
    position = get_first_non_whitespace_position(position, file);
    let mut end_node = root_node
        .descendant_for_point_range(position_to_point(position), position_to_point(position));
    while let Some(en) = end_node {
        if let Some(parent) = en.parent() {
            if parent.kind() == "body" {
                break;
            }
        }
        end_node = en.parent();
    }
    end_node
}

fn start_end_nodes_from_range<'a, 'b>(
    root_node: Node<'b>,
    mut range: Range,
    file: &'a str,
) -> Option<(Node<'b>, Node<'b>)> {
    if range.start.character == 0 && range.end.character == 0 {
        range.end.line -= 1;
    }
    let Some(start_node) = node_from_position(root_node, range.start, file) else {
        return None;
    };
    let Some(end_node) = node_from_position(root_node, range.end, file) else {
        return None;
    };
    Some((start_node, end_node))
}

fn nodes_from_range<'a, 'b>(
    root_node: Node<'b>,
    range: Range,
    file: &'a str,
) -> Option<Vec<Node<'b>>> {
    let Some((start_node, end_node)) = start_end_nodes_from_range(root_node, range, file) else {
        return None;
    };
    if start_node.parent() != end_node.parent() {
        return None;
    }
    let mut nodes = vec![start_node];
    let mut cur_node = start_node.next_sibling();
    while let Some(node) = cur_node {
        nodes.push(node);
        cur_node = node.next_sibling();
        if node == end_node {
            break;
        }
    }
    Some(nodes)
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::{Position, Range};

    use crate::{
        extract_into_function::{
            collect_non_declared_variables, collect_top_level_variable_definitions,
            nodes_from_range, start_end_nodes_from_range,
        },
        utils::node_content,
    };

    use super::{
        collect_used_variables, collect_variable_definitions, node_from_position, parse_file,
    };

    #[test]
    fn test_node_from_position() {
        let file = "func foo():
\tvar a = 10
\tvar b = a + 5
\tprint(a + b)
\tif a == 5:
\t\tvar c = a + b
\t\tprint(c)
\t\tvar add_speed_till_cap = capped_speed - cur_speed_in_direction
\t\tif a == b:
\t\t\tpass";
        let tree = parse_file(file).unwrap();
        let node = node_from_position(tree.root_node(), Position::new(1, 1), file).unwrap();
        assert_eq!(node_content(&node, file), "var a = 10");
        let node = node_from_position(tree.root_node(), Position::new(1, 10), file).unwrap();
        assert_eq!(node_content(&node, file), "var a = 10");
        let node = node_from_position(tree.root_node(), Position::new(3, 1), file).unwrap();
        assert_eq!(node_content(&node, file), "print(a + b)");
        let node = node_from_position(tree.root_node(), Position::new(3, 12), file).unwrap();
        assert_eq!(node_content(&node, file), "print(a + b)");
        let node = node_from_position(tree.root_node(), Position::new(1, 0), file).unwrap();
        assert_eq!(node_content(&node, file), "var a = 10");
        let node = node_from_position(tree.root_node(), Position::new(5, 0), file).unwrap();
        assert_eq!(node_content(&node, file), "var c = a + b");
        let node = node_from_position(tree.root_node(), Position::new(7, 64), file).unwrap();
        assert_eq!(
            node_content(&node, file),
            "var add_speed_till_cap = capped_speed - cur_speed_in_direction"
        );
        let node = node_from_position(tree.root_node(), Position::new(7, 0), file).unwrap();
        assert_eq!(
            node_content(&node, file),
            "var add_speed_till_cap = capped_speed - cur_speed_in_direction"
        );
    }

    #[test]
    fn test_node_from_range() {
        let file = "func foo():
\tvar a = 10
\tvar b = a + 5
\tprint(a + b)";
        let range = Range::new(Position::new(1, 5), Position::new(2, 6));
        let tree = parse_file(file).unwrap();
        let nodes = nodes_from_range(tree.root_node(), range, file).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_collect_variable_definitions() {
        let file = "func foo():
\tvar a = 10
\tvar b = 10
\tvar c = 10
\tvar d = 10
\tvar e = 10
\tprint(a + b)";
        let range = Range::new(Position::new(1, 0), Position::new(5, 0));
        let tree = parse_file(file).unwrap();
        let (start_node, end_node) =
            start_end_nodes_from_range(tree.root_node(), range, file).unwrap();
        let variables = collect_variable_definitions(start_node, end_node, file);
        assert_eq!(variables, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_collect_top_level_variable_definitions() {
        let file = "var top_level_a = 10
var top_level_b = 10
var top_level_c = 10

func foo():
\tvar a = 10
\tvar b = 10
\tvar c = 10
\tvar d = 10
\tvar e = 10
\tprint(a + b)

var top_level_d = 10";
        let tree = parse_file(file).unwrap();
        let variables = collect_top_level_variable_definitions(&tree, file);
        assert_eq!(
            variables,
            vec!["top_level_a", "top_level_b", "top_level_c", "top_level_d"]
        );
    }

    #[test]
    fn test_collect_used_variables() {
        let file = "func foo():
\tvar a = b + c + foo(d)";
        let tree = parse_file(file).unwrap();
        let node = node_from_position(tree.root_node(), Position::new(1, 2), file).unwrap();
        let mut used_variables = collect_used_variables(node, file)
            .into_iter()
            .collect::<Vec<_>>();
        used_variables.sort();
        assert_eq!(used_variables, vec!["b", "c", "d"]);
    }

    #[test]
    fn test_collect_non_declared_variables() {
        let file = "var b = 10
func foo():
\tvar c = 10
\tvar a = transform.basis + b + c + foo(d)";
        let tree = parse_file(file).unwrap();
        let start_node = node_from_position(tree.root_node(), Position::new(2, 2), file).unwrap();
        let end_node = node_from_position(tree.root_node(), Position::new(3, 2), file).unwrap();
        let mut used_variables = collect_non_declared_variables(&tree, start_node, end_node, file)
            .into_iter()
            .collect::<Vec<_>>();
        used_variables.sort();
        assert_eq!(used_variables, vec!["d", "transform"]);
    }
}
