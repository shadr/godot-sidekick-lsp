use std::collections::HashMap;

use tower_lsp::lsp_types::Position;
use tree_sitter::{Node, Tree};

use crate::utils::{node_content, point_to_position};

#[derive(Default, Debug)]
pub struct SymbolTable {
    pub map: HashMap<usize, Scope>,
    pub types: HashMap<String, TypeInfo>,
    pub functions: HashMap<String, MethodInfo>,
}

#[derive(Debug)]
pub struct TypeInfo {
    pub name: String,
    pub methods: HashMap<String, MethodInfo>,
    pub fields: HashMap<String, FieldInfo>,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub return_type: SymbolType,
}

#[derive(Debug)]
pub struct FieldInfo {
    ttype: SymbolType,
}

fn builtin_classes() -> HashMap<String, TypeInfo> {
    let mut map = HashMap::new();

    map.insert(
        "Vector3".to_string(),
        TypeInfo {
            name: "Vector3".to_string(),
            fields: vec![
                (
                    "x".to_string(),
                    FieldInfo {
                        ttype: SymbolType::Float,
                    },
                ),
                (
                    "y".to_string(),
                    FieldInfo {
                        ttype: SymbolType::Float,
                    },
                ),
                (
                    "z".to_string(),
                    FieldInfo {
                        ttype: SymbolType::Float,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            methods: vec![
                (
                    "normalized".to_string(),
                    MethodInfo {
                        return_type: SymbolType::Vector3,
                    },
                ),
                (
                    "length".to_string(),
                    MethodInfo {
                        return_type: SymbolType::Float,
                    },
                ),
                (
                    "dot".to_string(),
                    MethodInfo {
                        return_type: SymbolType::Float,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        },
    );

    map
}

#[derive(Debug)]
pub struct Scope {
    pub id: usize,
    pub parent: usize,
    pub vars: Vec<Symbol>,
}

impl Scope {
    pub fn new(node: Node, parent: usize) -> Self {
        Self {
            id: node.id(),
            parent,
            vars: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Symbol {
    pub name: String,
    pub byte: usize,
    pub hint_position: Position,
    pub static_typed: bool,
    pub ttype: SymbolType,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SymbolType {
    None,
    Unknown,
    Int,
    Float,
    String,
    Vector3,
}
impl SymbolType {
    fn from_name(name: &str) -> SymbolType {
        match name {
            "float" => Self::Float,
            "int" => Self::Int,
            "String" => Self::String,
            "Vector3" => Self::Vector3,
            _ => Self::None,
        }
    }
}

impl SymbolTable {
    pub fn build_table(&mut self, tree: &Tree, file: &str) {
        self.types = builtin_classes();

        let root = tree.root_node();
        self.map.insert(root.id(), Scope::new(root, 0));
        self.build_body(root, file, root.id());
        for symbol in &mut self.map.get_mut(&root.id()).unwrap().vars {
            symbol.byte = 0
        }
    }

    pub fn build_body(&mut self, body: Node, file: &str, scope_id: usize) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "variable_statement" | "const_statement" => {
                    let name_node = child.child_by_field_name("name").unwrap();
                    let name = node_content(&name_node, file);
                    let value_node = child.child_by_field_name("value");
                    let type_node = child.child_by_field_name("type");
                    let mut static_typed = false;
                    let mut ttype = SymbolType::None;
                    if let Some(type_node) = type_node {
                        ttype = SymbolType::from_name(node_content(&type_node, file));
                        static_typed = true;
                    } else if let Some(value_node) = value_node {
                        ttype = self.infer_type(scope_id, value_node, file)
                    }
                    let symbol = Symbol {
                        name: name.to_string(),
                        byte: child.end_byte(),
                        hint_position: point_to_position(name_node.end_position()),
                        static_typed,
                        ttype,
                    };
                    self.map.get_mut(&scope_id).unwrap().vars.push(symbol)
                }
                "function_definition"
                | "if_statement"
                | "elif_clause"
                | "else_clause"
                | "for_statement" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    self.map.insert(body_node.id(), Scope::new(body, scope_id));
                    self.build_body(body_node, file, body_node.id());
                }
                _ => (),
            }
        }
    }

    pub fn infer_type(&self, scope_id: usize, node: Node, file: &str) -> SymbolType {
        let _position = node.start_byte();
        match node.kind() {
            "integer" => SymbolType::Int,
            "float" => SymbolType::Float,
            "binary_operator" => self.infer_binary_operator_type(scope_id, node, file),
            "identifier" => self.infer_identifier_type(scope_id, node, file),
            "attribute" => self.infer_attribute_type(scope_id, node, file),
            "call" => self.infer_call_type(scope_id, node, file),
            kind => {
                dbg!("infering types not implemented for {}", kind);
                SymbolType::None
            }
        }
    }

    pub fn infer_attribute_type(&self, scope_id: usize, node: Node, file: &str) -> SymbolType {
        let identifier_node = node.child(0).unwrap();
        let name = node_content(&identifier_node, file);
        let Some(object_type) = self.get_symbol(scope_id, name, identifier_node.start_byte())
        else {
            return SymbolType::None;
        };
        let Some(type_info) = self.types.get(&format!("{:?}", object_type.ttype)) else {
            return SymbolType::None;
        };
        let attribute_node = node.child(2).unwrap();
        match attribute_node.kind() {
            "identifier" => {
                let field_name = node_content(&attribute_node, file);
                let Some(field_info) = type_info.fields.get(field_name) else {
                    return SymbolType::None;
                };
                field_info.ttype
            }
            "attribute_call" => {
                let method_name_node = attribute_node.child(0).unwrap();
                let method_name = node_content(&method_name_node, file);
                let Some(method_info) = type_info.methods.get(method_name) else {
                    return SymbolType::None;
                };
                method_info.return_type
            }
            _ => unreachable!(),
        }
    }

    pub fn infer_binary_operator_type(
        &self,
        scope_id: usize,
        bin_op: Node,
        file: &str,
    ) -> SymbolType {
        let left_node = bin_op.child_by_field_name("left").unwrap();
        let right_node = bin_op.child_by_field_name("right").unwrap();
        let left_type = self.infer_type(scope_id, left_node, file);
        let right_type = self.infer_type(scope_id, right_node, file);
        if left_type == right_type {
            return left_type;
        }
        SymbolType::None
    }

    pub fn get_symbol(&self, scope: usize, symbol: &str, position: usize) -> Option<&Symbol> {
        let scope = self.map.get(&scope)?;
        for var in &scope.vars {
            if var.byte >= position {
                break;
            }
            if var.name == symbol {
                return Some(var);
            }
        }
        self.get_symbol(scope.parent, symbol, position)
    }

    fn infer_identifier_type(&self, scope_id: usize, identifier: Node, file: &str) -> SymbolType {
        let name = node_content(&identifier, file);
        let Some(symbol) = self.get_symbol(scope_id, name, identifier.start_byte()) else {
            return SymbolType::None;
        };
        symbol.ttype
    }

    fn infer_call_type(&self, scope_id: usize, node: Node, file: &str) -> SymbolType {
        let name_node = node.child(0).unwrap();
        let name = node_content(&name_node, file);
        match name {
            "max" => {
                let Some(arguments) = node.child_by_field_name("arguments") else {
                    return SymbolType::None;
                };
                let Some(first_child) = arguments.child(1) else {
                    return SymbolType::None;
                };
                self.infer_type(scope_id, first_child, file)
            }
            _ => SymbolType::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::parse_file;

    use super::SymbolTable;

    #[test]
    fn simple() {
        let file = "var b = 10
func foo():
\tvar c: Vector3 = Vector3.ZERO
\tvar e = c.normalized()
\tvar f = max(e.x, 0.0)";
        let tree = parse_file(file).unwrap();
        let mut st = SymbolTable::default();
        st.build_table(&tree, file);
        dbg!(st);
        assert!(false);
    }
}
