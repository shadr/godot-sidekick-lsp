use std::{collections::HashMap, str::FromStr};

use tower_lsp::lsp_types::Position;
use tree_sitter::{Node, Tree};

use crate::{
    typedb::{SymbolType, TypeDatabase, VariantType},
    utils::{node_content, parse_file, point_to_position},
};

pub struct SymbolTable<'a> {
    pub map: HashMap<usize, Scope>,
    class_parent: Option<SymbolType>,
    typedb: &'a TypeDatabase,
}

#[derive(Debug)]
pub struct Scope {
    pub id: usize,
    pub parent: usize,
    pub vars: Vec<Symbol>,
}

impl Scope {
    pub fn new(node: Node, parent_scope: usize) -> Self {
        Self {
            id: node.id(),
            parent: parent_scope,
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
    pub ttype: Option<SymbolType>,
}

impl<'a> SymbolTable<'a> {
    pub fn new(typedb: &'a TypeDatabase) -> Self {
        Self {
            map: HashMap::new(),
            class_parent: None,
            typedb,
        }
    }

    pub fn build_table(&mut self, tree: &Tree, file: &str) {
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
                    let mut ttype = None;
                    if let Some(type_node) = type_node {
                        ttype = SymbolType::from_str(node_content(&type_node, file)).ok();
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
                "function_definition" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    self.map.insert(body_node.id(), Scope::new(body, scope_id));

                    if let Some(parameters) = child.child_by_field_name("parameters") {
                        let function_begins = body_node.start_byte();
                        for i in 0..parameters.child_count() {
                            let parameter = parameters.child(i).unwrap();
                            if parameter.kind() == "typed_parameter" {
                                let name_node = parameter.child(0).unwrap();
                                let name = node_content(&name_node, file);
                                let type_node = parameter.child_by_field_name("type");
                                let mut ttype = None;
                                if let Some(type_node) = type_node {
                                    ttype =
                                        SymbolType::from_str(node_content(&type_node, file)).ok();
                                }
                                let symbol = Symbol {
                                    name: name.to_string(),
                                    byte: function_begins,
                                    hint_position: point_to_position(name_node.end_position()),
                                    static_typed: true,
                                    ttype,
                                };
                                self.map.get_mut(&body_node.id()).unwrap().vars.push(symbol)
                            }
                        }
                    }

                    self.build_body(body_node, file, body_node.id());
                }
                "if_statement" | "elif_clause" | "else_clause" | "for_statement" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    self.map.insert(body_node.id(), Scope::new(body, scope_id));
                    self.build_body(body_node, file, body_node.id());
                }
                "extends_statement" => {
                    let type_node = child.child(1);
                    let mut ttype = None;
                    if let Some(type_node) = type_node {
                        ttype = SymbolType::from_str(node_content(&type_node, file)).ok();
                    }
                    self.class_parent = ttype;
                }
                _ => (),
            }
        }
    }

    pub fn infer_type(&self, scope_id: usize, node: Node, file: &str) -> Option<SymbolType> {
        let _position = node.start_byte();
        dbg!(node.to_sexp());
        match node.kind() {
            "integer" => Some(SymbolType::Variant(VariantType::Int)),
            "float" => Some(SymbolType::Variant(VariantType::Float)),
            "binary_operator" => self.infer_binary_operator_type(scope_id, node, file),
            "identifier" => self.infer_identifier_type(scope_id, node, file),
            "attribute" => self.infer_attribute_type(scope_id, node, file),
            "call" => self.infer_call_type(scope_id, node, file),
            _ => None,
        }
    }

    pub fn infer_attribute_type(
        &self,
        scope_id: usize,
        node: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let identifier_node = node.child(0).unwrap();
        let name = node_content(&identifier_node, file);
        let is_class;
        let type_info = if let Some(ttype) =
            self.get_symbol_type(scope_id, name, identifier_node.start_byte())
        {
            is_class = false;
            self.typedb.classes.get(&ttype.to_string())?
        } else {
            is_class = true;
            self.typedb.classes.get(name)?
        };
        let attribute_node = node.child(2).unwrap();
        match attribute_node.kind() {
            "identifier" => {
                let field_name = node_content(&attribute_node, file);
                if is_class {
                    let constant = type_info.constants.get(field_name)?;
                    // TODO: optimize this, currently we are parsing small value string like "Vector3(0.0, 0.0, 0.0)" using tree-sitter
                    // each time we want to infer type of constant like Vector3.ZERO
                    let parsed = parse_file(&constant.value)?;
                    let expression_statement = parsed.root_node().child(0)?;
                    let expression = expression_statement.child(0)?;
                    self.infer_type(0, expression, &constant.value)
                } else {
                    let field_info = type_info.properties.get(field_name)?;
                    Some(field_info.ttype.clone())
                }
            }
            "attribute_call" => {
                let method_name_node = attribute_node.child(0).unwrap();
                let method_name = node_content(&method_name_node, file);
                let method_info = type_info.methods.get(method_name)?;
                Some(method_info.return_type.clone())
            }
            _ => unreachable!(),
        }
    }

    pub fn infer_binary_operator_type(
        &self,
        scope_id: usize,
        bin_op: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let left_node = bin_op.child_by_field_name("left").unwrap();
        let right_node = bin_op.child_by_field_name("right").unwrap();
        let left_type = self.infer_type(scope_id, left_node, file);
        let right_type = self.infer_type(scope_id, right_node, file);
        if left_type == right_type {
            return left_type;
        }
        None
    }

    pub fn get_symbol_type(
        &self,
        scope: usize,
        symbol: &str,
        position: usize,
    ) -> Option<&SymbolType> {
        if scope == 0 {
            if let Some(parent) = &self.class_parent {
                let type_string = parent.to_string();
                return self.typedb.get_symbol_type(&type_string, symbol);
            }
        }
        let scope = self.map.get(&scope)?;
        for var in &scope.vars {
            if var.byte >= position {
                break;
            }
            if var.name == symbol {
                return var.ttype.as_ref();
            }
        }
        self.get_symbol_type(scope.parent, symbol, position)
    }

    fn infer_identifier_type(
        &self,
        scope_id: usize,
        identifier: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let name = node_content(&identifier, file);
        self.get_symbol_type(scope_id, name, identifier.start_byte())
            .cloned()
    }

    fn infer_call_type(&self, scope_id: usize, node: Node, file: &str) -> Option<SymbolType> {
        let name_node = node.child(0).unwrap();
        let name = node_content(&name_node, file);

        // If function names is equal to the name of one of the registered classes
        // then get constructor's return type
        if let Some(class) = self.typedb.classes.get(name) {
            if let Some(constructor) = &class.constructor {
                return Some(constructor.return_type.clone());
            }
        }

        // TODO: get local defined methods first
        if let Some(parent) = &self.class_parent {
            let type_string = parent.to_string();
            self.typedb.get_callable_type(&type_string, name).cloned()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{typedb::TypeDatabase, utils::parse_file};

    use super::SymbolTable;

    #[test]
    fn simple() {
        let file = "extends CharacterBody2D
    func foo(delta: float):
    \tvar a = Input.get_vector()
    \tvar b = Vector3.ZERO
    \tvar c = Vector3(1.0, 2.0, 3.0)
    \tvar d = get_slide_collision_count()";
        let tree = parse_file(file).unwrap();
        let typedb = TypeDatabase::from_file("./assets/type_info.json").unwrap();
        let mut st = SymbolTable::new(&typedb);
        st.build_table(&tree, file);
        dbg!(st.class_parent);
        dbg!(st.map);
        assert!(false);
    }
}
