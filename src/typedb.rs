use std::{collections::HashMap, str::FromStr};

use num_enum::{FromPrimitive, IntoPrimitive};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

pub struct TypeDatabase {
    pub classes: HashMap<String, ClassInfo>,
}

impl TypeDatabase {
    pub fn from_file(path: &str) -> Option<Self> {
        let content = std::fs::read_to_string(path).unwrap();
        Self::from_str(&content)
    }

    pub fn from_str(content: &str) -> Option<Self> {
        let json: HashMap<String, ClassInfoJson> = serde_json::from_str(&content).unwrap();
        let mut classes = HashMap::new();
        for (class_name, class) in json {
            let methods = class
                .methods
                .into_iter()
                .map(|m| {
                    (
                        m.name,
                        MethodInfo {
                            return_type: SymbolType::from_str(&m.return_type).unwrap(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>();

            let properties = class
                .properties
                .into_iter()
                .map(|m| {
                    (
                        m.name,
                        PropertyInfo {
                            ttype: SymbolType::from_str(&m.ttype).unwrap(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>();

            let mut constructor = None;
            if let Some(constr) = class.constructors.get(0) {
                constructor = Some(Constructor {
                    return_type: SymbolType::from_str(&constr.return_type).unwrap(),
                });
            }

            let constants = class
                .constants
                .into_iter()
                .map(|c| (c.name, Constant { value: c.value }))
                .collect::<HashMap<_, _>>();

            classes.insert(
                class_name.clone(),
                ClassInfo {
                    methods,
                    properties,
                    parent: class.parent,
                    constructor,
                    constants,
                },
            );
        }
        Some(Self { classes })
    }

    pub fn get_symbol_type(&self, class: &str, symbol: &str) -> Option<&SymbolType> {
        if let Some(class) = self.classes.get(class) {
            if let Some(prop) = class.properties.get(symbol) {
                return Some(&prop.ttype);
            }
            if let Some(parent_class) = &class.parent {
                return self.get_symbol_type(parent_class, symbol);
            }
        }
        None
    }

    // Get callable return type in specified class or its ancestors or in @GlobalScope
    pub fn get_callable_type(&self, class: &str, callable: &str) -> Option<&SymbolType> {
        if let Some(class) = self.classes.get(class) {
            if let Some(prop) = class.methods.get(callable) {
                return Some(&prop.return_type);
            }
            if let Some(parent_class) = &class.parent {
                return self.get_callable_type(parent_class, callable);
            }
        }
        if class != "@GlobalScope" {
            self.get_callable_type("@GlobalScope", callable)
        } else {
            None
        }
    }

    pub fn get_constant_type(&self, class: &str, const_name: &str) -> Option<SymbolType> {
        todo!()
    }
}

#[derive(Debug)]
pub struct ClassInfo {
    pub methods: HashMap<String, MethodInfo>,
    pub properties: HashMap<String, PropertyInfo>,
    pub parent: Option<String>,
    pub constructor: Option<Constructor>,
    pub constants: HashMap<String, Constant>,
}

#[derive(Debug)]
pub struct Constructor {
    pub return_type: SymbolType,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub return_type: SymbolType,
}

#[derive(Debug)]
pub struct PropertyInfo {
    pub ttype: SymbolType,
}

#[derive(Debug)]
pub struct Constant {
    pub value: String,
}

#[derive(Deserialize)]
struct ClassInfoJson {
    name: String,
    methods: Vec<MethodInfoJson>,
    parent: Option<String>,
    properties: Vec<PropertyInfoJson>,
    constructors: Vec<MethodInfoJson>,
    constants: Vec<ConstantJson>,
}

#[derive(Deserialize)]
struct MethodInfoJson {
    name: String,
    return_type: String,
    parameters: Vec<MethodParameterJson>,
}

#[derive(Deserialize)]
struct MethodParameterJson {
    name: String,
    #[serde(rename = "type")]
    ttype: String,
}

#[derive(Deserialize)]
struct PropertyInfoJson {
    name: String,
    #[serde(rename = "type")]
    ttype: String,
}

#[derive(Deserialize)]
struct ConstantJson {
    name: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolType {
    Variant(VariantType),
    Array(VariantType),
    Object(String),
    OjbectArray(String),
}

impl ToString for SymbolType {
    fn to_string(&self) -> String {
        match self {
            SymbolType::Variant(variant_type) => variant_type.to_string(),
            SymbolType::Array(variant_type) => format!("{}[]", variant_type.to_string()),
            SymbolType::Object(name) => name.clone(),
            SymbolType::OjbectArray(el_name) => format!("{}[]", el_name),
        }
    }
}

impl FromStr for SymbolType {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(array_element_type) = s.strip_suffix("[]") {
            match VariantType::from_str(array_element_type) {
                Ok(v) => Ok(Self::Array(v)),
                Err(strum::ParseError::VariantNotFound) => {
                    Ok(Self::OjbectArray(array_element_type.to_string()))
                }
            }
        } else {
            match VariantType::from_str(s) {
                Ok(v) => Ok(Self::Variant(v)),
                Err(strum::ParseError::VariantNotFound) => Ok(Self::Object(s.to_string())),
            }
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, IntoPrimitive, FromPrimitive, Clone, Copy, PartialEq, Eq, EnumString, Display)]
#[repr(u8)]
pub enum VariantType {
    #[num_enum(default)]
    #[strum(serialize = "void")]
    Nil = 0,
    #[strum(serialize = "bool")]
    Bool = 1,
    #[strum(serialize = "int")]
    Int = 2,
    #[strum(serialize = "float")]
    Float = 3,
    String = 4,
    Vector2 = 5,
    Vector2i = 6,
    Rect2 = 7,
    Rect2i = 8,
    Vector3 = 9,
    Vector3i = 10,
    Transform2d = 11,
    Vector4 = 12,
    Vector4i = 13,
    Plane = 14,
    Quaternion = 15,
    Aabb = 16,
    Basis = 17,
    Transform3d = 18,
    Projection = 19,
    Color = 20,
    String_name = 21,
    Node_path = 22,
    Rid = 23,
    Object = 24,
    Callable = 25,
    Signal = 26,
    Dictionary = 27,
    Array = 28,
    PackedByteArray = 29,
    PackedInt32Array = 30,
    PackedInt64Array = 31,
    PackedFloat32Array = 32,
    PackedFloat64Array = 33,
    PackedStringArray = 34,
    PackedVector2Array = 35,
    PackedVector3Array = 36,
    PackedColorArray = 37,
    PackedVector4Array = 38,
}

impl<'de> Deserialize<'de> for VariantType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let num: u8 = u8::deserialize(deserializer)?;
        Ok(Self::from_primitive(num))
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct PropertyUsage: u32 {
        const Storage = 2;
        const Editor = 4;
        const Internal = 8;
        const Checkable = 16;
        const Checked = 32;
        const Group = 64;
        const Category = 128;
        const Subgroup = 256;
        const ClassIsBitfield = 512;
        const NoInstanceState = 1024;
        const RestartIfChanged = 2048;
        const ScriptVariable = 4096;
        const StoreIfNull = 8192;
        const UpdateAllIfModified = 16384;
        const ScriptDefaultValue = 32768;
        const ClassIsEnum = 65536;
        const NilIsVariant = 131072;
        const Array = 262144;
        const AlwaysDuplicate = 524288;
        const NeverDuplicate = 1048576;
        const HighEndGfx = 2097152;
        const NodePathFromSceneRoot = 4194304;
        const ResourceNotPersistent = 8388608;
        const KeyingIncrements = 16777216;
        const DeferredSetResource = 33554432;
        const EditorInstantiateObject = 67108864;
        const EditorBasicSetting = 134217728;
        const ReadOnly = 268435456;
        const Secret = 536870912;
    }
}

impl Serialize for PropertyUsage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.bits())
    }
}

impl<'de> Deserialize<'de> for PropertyUsage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let num: u32 = u32::deserialize(deserializer)?;
        Ok(Self::from_bits_truncate(num))
    }
}

#[cfg(test)]
mod tests {
    use super::TypeDatabase;

    #[test]
    fn read_type_info_file() {
        TypeDatabase::from_file("./assets/type_info.json").unwrap();
    }
}
