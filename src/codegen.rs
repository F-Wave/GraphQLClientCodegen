use crate::schema;
use crate::parser;
use crate::parser::Type;
use crate::schema::NamedTypeKind;
use std::io::Write;
use std::fs;
use std::collections::{HashMap, HashSet};

pub struct Codegen<'a> {
    fragments_on: &'a HashMap<String, String>,
    schema: &'a schema::Schema,
    src: String,
    indent: usize,
}

enum TypeCase<'a> {
    SoleFragment(&'a str),
    InterfaceOnlyFragments,
    Interface,
    Regular,
}

impl<'a> Codegen<'a> {
    fn indent(&mut self) {
        self.indent += 1;
    }

    fn unindent(&mut self) {
        self.indent -= 1;
    }

    fn opening_brace(&mut self) {
        self.src += " {";
        self.indent();
    }

    fn closing_brace(&mut self) {
        self.unindent();
        self.newline();
        self.src += "}";
    }

    fn newline(&mut self) {
        self.src += "\n";
        for i in 0..self.indent {
            self.src += "    ";
        }
    }

    pub fn swift_name(s: &str) -> String {
        let mut c = s.chars();
        c.next().unwrap().to_uppercase().chain(c).collect()
    }

    fn write_type_non_nullable(&mut self, of_type: &parser::Type, fields: &Vec<parser::Field>, nest_type: &str) {
        match of_type {
            parser::Type::NonNull(_) => panic!("unexpected non null!"),
            parser::Type::String => self.src += "String",
            parser::Type::Int => self.src += "Int",
            parser::Type::Float => self.src += "Float",
            parser::Type::Bool => self.src += "Bool",
            parser::Type::Input(name) => {
                let kind = &self.schema.get(name).unwrap().kind;
                if *kind == NamedTypeKind::InputObject || *kind == NamedTypeKind::Scalar {
                    return self.src += name;
                }
                if let Some(frag) = self.sole_fragment(fields) {
                    return self.src += &Self::swift_name(frag);
                }
                self.src += nest_type
            },
            parser::Type::Array(elem) => {
                self.src += "[";
                self.write_type(elem, fields, nest_type);
                self.src += "]";
            }
        }
    }

    fn write_type(&mut self, of_type: &parser::Type, fields: &Vec<parser::Field>, nest_type: &str) {
        match of_type {
            parser::Type::NonNull(of_type) => self.write_type_non_nullable(of_type.as_ref(), fields, nest_type),
            _ => {
                self.write_type_non_nullable(of_type, fields, nest_type);
                self.src += "?";
            }
        }
    }


    fn sole_fragment(&self, fields: &Vec<parser::Field<'a>>) -> Option<&'a str> {
        if fields.len() == 1 {
            if let parser::Field::Fragment(frag) = fields[0] { return Some(frag); }
        }
        None
    }

    fn has_only_fragments(&self, fields: &Vec<parser::Field>) -> bool {
        for field in fields {
            if let parser::Field::PlainField(_) = field { return false }
        }
        true
    }

    fn anaylze(&self, named: &schema::NamedType, fields: &Vec<parser::Field<'a>>) -> TypeCase<'a> {
        if let Some(frag) = self.sole_fragment(fields) { return TypeCase::SoleFragment(frag) }

        if named.kind == NamedTypeKind::Interface {
            if self.has_only_fragments(fields) { TypeCase::InterfaceOnlyFragments }
            else { TypeCase::Interface }
        }  else {
            TypeCase::Regular
        }
    }

    fn should_gen_nested_types(&self, fields: &Vec<parser::Field<'a>>) -> bool {
        fields.len() > 0 && self.sole_fragment(&fields).is_none()
    }
    /*
    fn inline_frag_type(&mut self) {
        if inline.fields.len() == 1{
            if let Some(frag) = inline.fields[0] {

            }
        }
        self.src += &format!("case As{}({})", name, name);
    }*/


    fn gen_custom_interface_decoding(&mut self, object_type: &schema::NamedType, fields: &Vec<parser::Field>) {
        self.newline();
        self.src += "enum CodingKeys : String, CodingKey";
        self.opening_brace();
        self.newline();
        self.src += "case __typename";

        for field in fields {
            if let parser::Field::PlainField(field) = field {
                self.src += ", ";
                self.src += field.name;
            }
        }

        self.closing_brace();
        self.newline();

        self.src += "init(from decoder: Decoder) throws";
        self.opening_brace();
        self.newline();
        self.src += "let container = try decoder.container(keyedBy: CodingKeys.self)";
        for field in fields {
            if let parser::Field::PlainField(field) = field {
                self.newline();
                self.src += "self.";
                self.src += field.name;
                self.src += " = try container.decode(";
                self.write_type(&object_type.fields[field.name].of_type, &field.fields, field.name);
                self.src += ".self, forKey: .";
                self.src += field.name;
                self.src += ")";
                //id = try container.decode(String.self, forKey: .id)
                //mfgInfo = try MfgInfo(from: decoder)
            }
        }
        self.newline();
        self.src += "self.kind = try Types(from: decoder)";
        self.closing_brace();
    }


    fn gen_enum_for_possible_types(&mut self, object_type: &schema::NamedType, name: &str, fields: &Vec<parser::Field<'a>>) {
        let is_identifiable = object_type.fields.get("id").is_some();
        self.newline();
        self.gen_type_def("enum", name, is_identifiable);

        let mut cases = vec![];

        for field in fields {
            let (name, of_type) = match field {
                parser::Field::Fragment(name) => (*name, *name), //todo get named on
                parser::Field::InlineFragment(inline) => {
                    let name : &str = &self.schema.get_named(&inline.on).name;
                    if let Some(frag) = self.sole_fragment(&inline.fields) {
                        (name, frag)
                    } else {
                        (name, name)
                    }
                },
                _ => {continue},
            };

            self.newline();
            let type_name = Self::swift_name(of_type);
            self.src += &format!("case As{}({})", name, &type_name);
            cases.push((name, type_name));
        }

        if is_identifiable {
            self.newline();
            self.src += "var id : Int ";
            self.opening_brace();
            self.newline();
            self.src += "switch self";
            self.opening_brace();

            for (name, of_type) in &cases {
                self.newline();
                self.src += "case let .As";
                self.src += name;
                self.src += "(value) : return value.id"
            }
            self.closing_brace();
            self.closing_brace();
        }

        self.newline();
        self.src += "init(from decoder: Decoder) throws";
        self.opening_brace();
        self.newline();
        self.src += "let container = try decoder.container(keyedBy: TypenameKeys.self)";
        self.newline();
        self.src += "switch try container.decode(String.self, forKey: .__typename)";
        self.opening_brace();

        for (name, of_type) in &cases {
            self.newline();
            self.src += "case \"";
            self.src += name;
            self.src += "\" : self = .As";
            self.src += name;
            self.src += "(try ";
            self.src += &of_type;
            self.src += "(from: decoder))";
        }
        self.newline();
        self.src += "default: throw UnknownTypename()";

        self.closing_brace();
        self.closing_brace();


        self.closing_brace();
        self.newline();
    }

    fn gen_type_def(&mut self, kind: &str, name: &str, is_identifiable: bool) {
        self.src += kind;
        self.src += " ";
        self.src += &Self::swift_name(name);
        self.src += " : Decodable";
        if is_identifiable {
            self.src += ", Identifiable";
        }
        self.opening_brace();
    }

    fn has_id_field(&self, fields: &Vec<parser::Field<'a>>) -> bool {
        for field in fields {
            match field {
                parser::Field::PlainField(field) if field.name == "id" => return true,
                _ => {}
            }
        }

        false
    }


    fn gen_type_for_fields(&mut self, object_type: &schema::NamedType, is_interface: bool, fields: &Vec<parser::Field<'a>>) {
        for field in fields {
            match field {
                parser::Field::PlainField(field) => {
                    if self.should_gen_nested_types(&field.fields) {
                        let named = self.schema.get_type_of_field(object_type, field.name);

                        self.gen_type_for(named, field.name, &field.fields);
                    }
                },
                parser::Field::InlineFragment(inline) => {
                    if is_interface { return }

                    let on_type = self.schema.get_named(&inline.on);
                    self.gen_type_for(on_type, &on_type.name, &inline.fields);
                }
                _ => {},
            }
        }
    }

    fn gen_type_for(&mut self, object_type: &schema::NamedType, name: &'a str, fields: &Vec<parser::Field<'a>>) -> &'a str {
        match self.anaylze(object_type, fields) {
            TypeCase::SoleFragment(name) => return name,
            TypeCase::InterfaceOnlyFragments => {
                self.newline();
                self.gen_enum_for_possible_types(object_type, name, fields);
            }
            TypeCase::Interface => {
                self.newline();
                self.gen_type_def("struct", name, self.has_id_field(fields));
                self.gen_enum_for_possible_types(object_type, "Types", fields);
                self.gen_type_for_fields(object_type, true, fields);
                self.gen_fields(object_type, fields);
                self.gen_custom_interface_decoding(object_type, fields);
                self.closing_brace();
            }

            TypeCase::Regular => {
                self.newline();
                self.gen_type_def("struct", name, self.has_id_field(fields));
                self.gen_type_for_fields(object_type, false, fields);
                self.gen_fields(object_type, fields);
                self.closing_brace();
            },
        }

        name
    }

    fn gen_fields(&mut self, object_type: &schema::NamedType, fields: &Vec<parser::Field<'a>>) {
        let is_interface = object_type.kind == NamedTypeKind::Interface;

        for field in fields {
            match field {
                parser::Field::PlainField(field) => {
                    self.newline();
                    let schema_field = &object_type.fields[field.name];
                    self.src += "var ";
                    self.src += field.name;
                    self.src += " : ";

                    let nest_type = Self::swift_name(field.name);
                    self.write_type(&schema_field.of_type, &field.fields, &nest_type);
                },
                parser::Field::InlineFragment(_frag) => {

                },
                parser::Field::Fragment(frag) => {
                    if !is_interface {
                        self.src += "var ";
                        self.src += frag;
                        self.src += " : ";
                        self.src += &Self::swift_name(frag);
                    }
                },
            }
        }

        if is_interface {
            self.newline();
            self.src += "var kind : Types"
        }
    }

    fn gen_args(&mut self, args: &Vec<parser::ArgumentDef<'a>>) {
        for arg in args {
            self.src += "var ";
            self.src += arg.name;
            self.src += " : ";
            self.write_type(&arg.kind, &vec![], "");
            self.newline();
        }
    }

    fn gen_ql_value(&mut self, value: &parser::Value) {
        match value {
            parser::Value::Bool(b)  => if *b { self.src += "true" } else { self.src += "false " },
            parser::Value::String(s) => {
                self.src += "\"";
                self.src += s;
                self.src += "\"";
            },
            parser::Value::Int(i) => self.src += &format!("{} ", i),
            parser::Value::Variable(name) => {
                self.src += "$";
                self.src += name;
            }
        }
    }

    fn gen_ql_type(&mut self, of_type: &parser::Type) {
        match of_type {
            Type::String => self.src += "String",
            Type::Float => self.src += "Float",
            Type::Bool => self.src += "Bool",
            Type::Int => self.src += "Int",
            Type::NonNull(elem) => {
                self.gen_ql_type(elem);
                self.src += "!";
            }
            Type::Array(elem) => {
                self.src += "[";
                self.gen_ql_type(elem);
                self.src += "]";
            }
            Type::Input(input) => self.src += input
        }
    }

    fn comma_seperated<F: Fn(&mut Codegen<'a>, &T), T>(&mut self, vec: &Vec<T>, f: F) {
        for (i, arg) in vec.iter().enumerate() { //todo create helper which checks if last
            f(self, arg);
            if i + 1 < vec.len() {
                self.src += ", ";
            }
        }
    }

    fn gen_ql_fields(&mut self, object_type: &schema::NamedType, fields: &Vec<parser::Field<'a>>)  {
        if fields.len() == 0 { return }


        self.opening_brace();

        if object_type.kind == NamedTypeKind::Interface {
            self.newline();
            self.src += "__typename";
        }

        for field in fields {
            self.newline();
            match field {
                parser::Field::PlainField(plain_field) => {
                    self.src += plain_field.name;
                    if plain_field.args.len() > 0 {
                        self.src += "(";
                        self.comma_seperated(&plain_field.args, |codegen, arg| { //todo create helper which checks if last
                            codegen.src += arg.name;
                            codegen.src += " : ";
                            codegen.gen_ql_value(&arg.value);
                        });
                        self.src += ")";
                    }
                    if plain_field.fields.len() > 0 {
                        self.gen_ql_fields(self.schema.get_type_of_field(object_type, plain_field.name), &plain_field.fields);
                    }
                },
                parser::Field::Fragment(frag) => {
                    self.src += "...";
                    self.src += frag;
                },
                parser::Field::InlineFragment(inline) => {
                    self.src += "... on ";
                    self.gen_ql_type(&inline.on);
                    self.gen_ql_fields(self.schema.get_named(&inline.on), &inline.fields);
                },
            }
        }
        self.closing_brace();
    }

    fn find_fragments(fragments: &mut HashSet<&'a str>, fields: &Vec<parser::Field<'a>>) {
        for field in fields {
            match field {
                parser::Field::PlainField(field) => Self::find_fragments(fragments, &field.fields),
                parser::Field::InlineFragment(inline) => Self::find_fragments(fragments, &inline.fields),
                parser::Field::Fragment(on) => { fragments.insert(on); },
            }
        }
    }

    fn gen_dependent_fragments(&mut self, fields: &Vec<parser::Field<'a>>) {
        let mut fragments = HashSet::new();
        Self::find_fragments(&mut fragments, fields);

        self.src += "[";

        for (i, frag) in fragments.iter().enumerate() {
            self.src += "\"";
            self.src += frag;
            self.src += "\"";
            if i + 1 < fragments.len() { self.src += ","; }
        }

        self.src += "]";
    }

    fn gen_ql_args(&mut self, args: &Vec<parser::ArgumentDef<'a>>) {
        if args.len() > 0 {
            self.src += "(";
            self.comma_seperated(args,  |codegen, arg| {
                codegen.src += "$";
                codegen.src += arg.name;
                codegen.src += " : ";
                codegen.gen_ql_type(&arg.kind);
            });
            self.src += ")";
        }
    }

    fn gen_ql(&mut self, kind: &str, base: &schema::NamedType, name: &str, args: &Vec<parser::ArgumentDef<'a>>, fields: &Vec<parser::Field<'a>>) {
        self.src += "static let fragments : [String] = ";
        self.gen_dependent_fragments(fields);

        self.newline();

        self.src += "static let graphql = \"\"\"";
        self.newline();
        self.src += kind;
        self.src += " ";
        self.src += name;

        self.gen_ql_args(&args);
        self.gen_ql_fields(base, &fields);
        self.newline();
        self.src += "\"\"\"";
        self.newline();
    }

    fn gen_api_for(&mut self, kind: &str, base: &schema::NamedType, name: &str, args: &Vec<parser::ArgumentDef<'a>>, fields: &Vec<parser::Field<'a>>) {
        self.newline();
        self.newline();

        let kind_upper = Self::swift_name(kind);

        self.src += &format!("struct {}{} : Encodable, GraphQL{}", Self::swift_name(name), &kind_upper, &kind_upper);
        self.opening_brace();
        self.newline();
        self.gen_ql(kind, base, &name, &args, &fields);
        self.newline();
        self.gen_args(&args);

        self.gen_type_for(base, "Data", &fields);
        self.closing_brace();
    }

    fn gen_queries(&mut self, queries: &Vec<parser::Query<'a>>) {
        let schema = self.schema.query_root().unwrap();

        for query in queries {
            self.gen_api_for("query", schema, &query.name, &query.args, &query.fields);
        }
    }

    fn gen_mutations(&mut self, mutations: &Vec<parser::Mutation<'a>>) {
        let schema = self.schema.mutation_root().unwrap();

        for query in mutations {
            self.gen_api_for("mutation", schema, &query.name, &query.args, &query.fields);
        }
    }

    fn gen_fragments(&mut self, fragments: &Vec<parser::Fragment<'a>>) {

        for query in fragments {
            self.newline();
            self.newline();

            let schema = self.schema.get_named(&query.on);

            //self.src += &format!("struct {} : GraphQLFragment", Self::swift_name(query.name));
            //self.opening_brace();
            //self.newline();
            //let name = Self::swift_name(query.name);
            self.gen_type_for(schema, query.name, &query.fields);
            self.newline();
            self.src += "func init";
            self.src += &Self::swift_name(query.name);
            self.src += "Fragment(meta: FragmentMeta)";
            self.opening_brace();
            self.newline();
            self.src += "meta.register(name: \"";
            self.src += query.name;
            self.src += "\", fragments: ";
            self.gen_dependent_fragments(&query.fields);
            self.src += ", graphql: \"\"\"";
            self.newline();
            self.src += "fragment ";
            self.src += query.name;
            self.src += " on ";
            self.src += &schema.name;
            self.gen_ql_args(&query.args);
            self.gen_ql_fields(schema, &query.fields);
            self.newline();
            self.src += "\"\"\")";

            self.closing_brace();
        }
    }


}

pub fn gen(schema: &schema::Schema, module: &parser::GraphQL) -> String {
    let fragments_on = HashMap::new();
    let mut codegen = Codegen{ fragments_on: &fragments_on, schema, src: "".to_string(), indent: 0 };

    codegen.gen_fragments(&module.fragments);
    codegen.gen_queries(&module.queries);
    codegen.gen_mutations(&module.mutations);

    return codegen.src;
}