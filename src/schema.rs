use std::collections::HashMap;
use serde_json::{Value, Map};
use crate::parser::Type;

#[derive(PartialEq)]
pub enum NamedTypeKind {
    Scalar, Object, Enum, InputObject, Interface
}

pub struct Argument {
    pub name: String,
    pub of_type: Type,
}

pub struct Field {
    pub name: String,
    pub args: Vec<Argument>,
    pub of_type: Type,
}

pub struct NamedType {
    pub name: String,
    pub kind: NamedTypeKind,
    pub fields: HashMap<String, Field>
}

pub struct Schema {
    mutation_type: String,
    query_type: String,
    types: HashMap<String, NamedType>
}

fn type_from(of_type: &Map<String, Value>) -> Type {
    let name = of_type["name"].as_str();
    let kind = of_type["kind"].as_str().unwrap();

    match kind {
        "SCALAR" => match name.unwrap() {
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "String" => Type::String,
            _ => Type::Input(name.unwrap().to_string()),
        },
        "NON_NULL" => {
            let of_type = of_type["ofType"].as_object().unwrap();
            Type::NonNull(Box::new(type_from(of_type)))
        },
        "LIST" => {
            let of_type = of_type["ofType"].as_object().unwrap();
            Type::Array(Box::new(type_from(of_type)))
        },
        "ENUM" => {
            //let of_type = of_type["ofType"].as_object().unwrap();
            //Type::Enum(Box::new(type_from(of_type)))
            Type::Input(name.unwrap().to_string())
        }
        "OBJECT" | "INTERFACE" | "INPUT_OBJECT" => Type::Input(name.unwrap().to_string()),
        _ => panic!("Unknown kind {}", kind),
    }
}

fn map_array_object<F: Fn(&Map<String, Value>) -> T , T>(value: &Value, func: F) -> Vec<T> {
    let data = match value.as_array() {
        Some(data) => data,
        None => return vec![],
    };

    let mut result = Vec::with_capacity(data.len());
    for value in data {
        let obj = value.as_object().unwrap();
        result.push(func(obj));
    }
    result
}

fn args_from(args: &Value) -> Vec<Argument> {
    map_array_object(args, |arg| Argument {
        name: arg["name"].as_str().unwrap().to_string(),
        of_type: type_from(arg["type"].as_object().unwrap()),
    })
}

//todo perf
fn fields_from(fields: &Value) -> HashMap<String, Field> {
    let fields = map_array_object(fields, |field| Field {
        name: field["name"].as_str().unwrap().to_string(),
        args: args_from(&field["args"]),
        of_type: type_from(&field["type"].as_object().unwrap())
    });

    let mut result = HashMap::new();
    for field in fields {
        result.insert(field.name.clone(), field);
    }

    result
}

pub fn from(src: &str) -> Result<Schema, serde_json::Error> {
    let json_schema_resp : serde_json::Map<String, serde_json::Value> = serde_json::from_str(src)?;
    let json_schema = json_schema_resp["__schema"].as_object().unwrap();

    let query_type  = json_schema["queryType"].as_object().unwrap();
    let mutation_type = json_schema["mutationType"].as_object().unwrap();
    //let subscription_type = json_schema["subscriptionType"].as_object().unwrap();
    let types  = json_schema["types"].as_array().unwrap();

    let mut types_result = HashMap::new();

    for value in types {
        let of_type = value.as_object().unwrap();

        let name = of_type["name"].as_str().unwrap();
        let kind_str = of_type["kind"].as_str().unwrap();
        let kind = match kind_str {
            "OBJECT" => NamedTypeKind::Object,
            "INTERFACE" => NamedTypeKind::Interface,
            "SCALAR" => NamedTypeKind::Scalar,
            "INPUT_OBJECT" => NamedTypeKind::InputObject,
            "ENUM" => NamedTypeKind::Enum,
            _ => panic!("expecting object, interface, input object or scalar, not {}", kind_str)
        };
        let fields = fields_from(&of_type["fields"]);

        types_result.insert(name.to_string(), NamedType{
            name: name.to_string(),
            kind,
            fields: fields
        });
    }

    Ok(Schema{
        query_type: query_type["name"].as_str().unwrap().to_string(),
        mutation_type: mutation_type["name"].as_str().unwrap().to_string(),
        types: types_result
    })
}

impl Schema {
    pub fn get_named(&self, object_type: &Type) -> &NamedType {
        match object_type {
            Type::Input(name) => self.get(name).unwrap(),
            Type::Array(elem) => self.get_named(elem.as_ref()),
            Type::NonNull(elem) => self.get_named(elem.as_ref()),
            _ => panic!("Can only get field on interface, input or object types"),
        }
    }

    pub fn get_type_of_field(&self, object_type: &NamedType, name: &str) -> &NamedType {
        let of_type = &object_type.fields[name].of_type;
        return self.get_named(of_type);
    }

    pub fn get(&self, name: &str) -> Option<&NamedType> {
        self.types.get(name)
    }

    pub fn query_root(&self) -> Option<&NamedType> {
        self.types.get(&self.query_type)
    }

    pub fn mutation_root(&self) -> Option<&NamedType> {
        self.types.get(&self.mutation_type)
    }
}