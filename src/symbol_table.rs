use std::{collections::HashMap, str::FromStr};

use tower_lsp::lsp_types::Position;
use tree_sitter::{Node, Tree};

use crate::{
    typedb::{SymbolType, TypeDatabase, VariantType},
    utils::{node_content, parse_file, point_to_position},
};

// TODO: @GDScript (range, print functions etc)

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
        let new_scope_id = self.insert_new_scope(root, 0);
        self.build_body(root, file);
        for symbol in &mut self.map.get_mut(&new_scope_id).unwrap().vars {
            symbol.byte = 0
        }
    }

    pub fn insert_new_scope(&mut self, body: Node, parent_scope: usize) -> usize {
        let new_scope = Scope::new(body, parent_scope);
        let id = new_scope.id;
        self.map.insert(id, new_scope);
        id
    }

    pub fn build_body(&mut self, body: Node, file: &str) {
        let current_scope_id = body.id();
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
                        ttype = self.infer_type(current_scope_id, value_node, file)
                    }
                    let symbol = Symbol {
                        name: name.to_string(),
                        byte: child.end_byte(),
                        hint_position: point_to_position(name_node.end_position()),
                        static_typed,
                        ttype,
                    };
                    self.map
                        .get_mut(&current_scope_id)
                        .unwrap()
                        .vars
                        .push(symbol)
                }
                "function_definition" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    let new_scope_id = self.insert_new_scope(body_node, current_scope_id);

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
                                self.map.get_mut(&new_scope_id).unwrap().vars.push(symbol)
                            }
                        }
                    }

                    self.build_body(body_node, file);
                }
                "if_statement" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    self.insert_new_scope(body_node, current_scope_id);
                    self.build_body(body_node, file);

                    // process `elif_clause` and `else_clause`, they are exist in `alternative` field of an `if_statement` node
                    let mut cursor = child.walk();
                    let alternatives = child.children_by_field_name("alternative", &mut cursor);
                    for elif_clause in alternatives {
                        let body_node = elif_clause.child_by_field_name("body").unwrap();
                        self.insert_new_scope(body_node, current_scope_id);
                        self.build_body(body_node, file);
                    }
                }
                "elif_clause" | "else_clause" | "for_statement" => {
                    let body_node = child.child_by_field_name("body").unwrap();
                    self.insert_new_scope(body_node, current_scope_id);
                    self.build_body(body_node, file);
                }
                "extends_statement" => {
                    let type_node = child.child(1);
                    let mut ttype = None;
                    if let Some(type_node) = type_node {
                        ttype = SymbolType::from_str(node_content(&type_node, file)).ok();
                    }
                    self.class_parent = ttype;
                }
                "match_statement" => {
                    let Some(match_body) = child.child_by_field_name("body") else {
                        continue;
                    };
                    let mut cursor = match_body.walk();
                    for pattern_section in match_body.children(&mut cursor) {
                        let Some(pattern_body) = pattern_section.child_by_field_name("body") else {
                            continue;
                        };
                        self.insert_new_scope(pattern_body, current_scope_id);
                        self.build_body(pattern_body, file);
                    }
                }
                _ => (),
            }
        }
    }

    pub fn infer_type(&self, scope_id: usize, node: Node, file: &str) -> Option<SymbolType> {
        let _position = node.start_byte();
        match node.kind() {
            "integer" => Some(SymbolType::Variant(VariantType::Int)),
            "float" => Some(SymbolType::Variant(VariantType::Float)),
            "false" | "true" => Some(SymbolType::Variant(VariantType::Bool)),
            "string" => Some(SymbolType::Variant(VariantType::String)),
            "binary_operator" => self.infer_binary_operator_type(scope_id, node, file),
            "identifier" => self.infer_identifier_type(scope_id, node, file),
            "attribute" => self.infer_attribute_type(scope_id, node, file),
            "call" => self.infer_call_type(scope_id, node, file),
            "parenthesized_expression" => {
                self.infer_parenthesized_expression_type(scope_id, node, file)
            }
            "unary_operator" => self.infer_unary_operator_type(scope_id, node, file),
            _ => None,
        }
    }

    pub fn infer_attribute_type(
        &self,
        scope_id: usize,
        node: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let lhs_node = node.child(0).unwrap();
        let name = node_content(&lhs_node, file);
        let is_class;
        let type_info = if lhs_node.kind() == "parenthesized_expression" {
            // TODO: doesn't support parenthesized class names like "(Input).get_vector..."
            is_class = false;
            let paren_expr_type =
                self.infer_parenthesized_expression_type(scope_id, lhs_node, file)?;
            self.typedb.classes.get(&paren_expr_type.to_string())?
        } else if let Some(ttype) = self.get_symbol_type(scope_id, name, lhs_node.start_byte()) {
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
        let left_type = self.infer_type(scope_id, left_node, file)?;
        let right_type = self.infer_type(scope_id, right_node, file)?;
        let left_type_str = left_type.to_string();
        // TODO: should we use `node_content` instead of relying on the fact that kind is equal to operator character ?
        let op = bin_op.child(1)?.kind();
        self.typedb
            .get_binary_operator_type(&left_type_str, op, right_type)
            .cloned()
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
            let infered_type = self.typedb.get_callable_type(&type_string, name).cloned();
            if infered_type == Some(SymbolType::Object("Variant".to_string())) {
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    if let Some(first_child) = arguments.child(1) {
                        return self.infer_type(scope_id, first_child, file);
                    }
                }
            }
            infered_type
        } else {
            None
        }
    }

    fn infer_parenthesized_expression_type(
        &self,
        scope_id: usize,
        node: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let inner_expression = node.child(1)?;
        self.infer_type(scope_id, inner_expression, file)
    }

    fn infer_unary_operator_type(
        &self,
        scope_id: usize,
        node: Node,
        file: &str,
    ) -> Option<SymbolType> {
        let inner_expression = node.child(1)?;
        let inner_type = self.infer_type(scope_id, inner_expression, file)?;
        let inner_type_str = inner_type.to_string();
        let op = node.child(0)?.kind();
        self.typedb
            .get_unary_operator_type(&inner_type_str, op)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use tree_sitter::Tree;

    use crate::{
        typedb::{SymbolType, TypeDatabase, VariantType},
        utils::parse_file,
    };

    static TEST_TYPEDB: LazyLock<TypeDatabase> =
        LazyLock::new(|| TypeDatabase::from_file("./assets/type_info.json").unwrap());

    use super::SymbolTable;

    fn test_build_st(file: &str) -> (SymbolTable, Tree) {
        let tree = parse_file(file).unwrap();
        dbg!(tree.root_node().to_sexp());
        let mut st = SymbolTable::new(&TEST_TYPEDB);
        st.build_table(&tree, file);
        (st, tree)
    }

    /// Checks if variable in a function has specific type
    fn assert_var_type((st, tree): &(SymbolTable, Tree), var_name: &str, ty: SymbolType) {
        let root = tree.root_node();
        let mut function_node = root.child(0).unwrap();
        if function_node.kind() != "function_definition" {
            function_node = root.child(1).unwrap();
        }
        let function_scope_id = function_node.child_by_field_name("body").unwrap().id();
        let scope = st.map.get(&function_scope_id).unwrap();
        for var in &scope.vars {
            if var.name == var_name {
                assert_eq!(var.ttype, Some(ty.clone()));
            }
        }
    }

    #[test]
    fn simple_variable_assignments() {
        let file = "func foo():
\tvar b = false
\tvar i = 123
\tvar f = 42.0
\tvar s = \"hello\"";
        let st = test_build_st(file);
        assert_var_type(&st, "b", SymbolType::Variant(VariantType::Bool));
        assert_var_type(&st, "i", SymbolType::Variant(VariantType::Int));
        assert_var_type(&st, "f", SymbolType::Variant(VariantType::Float));
        assert_var_type(&st, "s", SymbolType::Variant(VariantType::String));
    }

    #[test]
    fn assign_constant_from_class() {
        let file = "func foo():
\tvar v = Vector3.ZERO";
        let st = test_build_st(file);
        assert_var_type(&st, "v", SymbolType::Variant(VariantType::Vector3));
    }

    #[test]
    fn assign_result_of_binary_operator() {
        let file = "func foo():
\tvar a = 123
\tvar b = 456
\tvar r = a + b";
        let st = test_build_st(file);
        assert_var_type(&st, "r", SymbolType::Variant(VariantType::Int));
    }

    #[test]
    fn assign_field_from_parent_class() {
        let file = "extends CharacterBody3D
func foo():
\tvar one_level = velocity
\tvar three_level = transform";
        let st = test_build_st(file);
        assert_var_type(&st, "one_level", SymbolType::Variant(VariantType::Vector3));
        assert_var_type(
            &st,
            "three_level",
            SymbolType::Variant(VariantType::Transform3d),
        );
    }
}
