use std::io::Write;

use syn::*;

use config::Config;
use bindgen::library::*;
use bindgen::syn_helpers::*;

#[derive(Debug, Clone)]
pub enum Type {
    ConstPtr(Box<Type>),
    Ptr(Box<Type>),
    Path(PathRef),
    Primitive(String),
    Array(Box<Type>, u64),
    FuncPtr(Option<Box<Type>>, Vec<Type>),
}
impl Type {
    pub fn convert(ty: &Ty) -> ConvertResult<Type> {
        match ty {
            &Ty::Rptr(_, ref mut_ty) => {
                let converted = try!(Type::convert(&mut_ty.ty));

                Ok(match mut_ty.mutability {
                    Mutability::Mutable => Type::Ptr(Box::new(converted)),
                    Mutability::Immutable => Type::ConstPtr(Box::new(converted)),
                })
            }
            &Ty::Ptr(ref mut_ty) => {
                let converted = try!(Type::convert(&mut_ty.ty));

                Ok(match mut_ty.mutability {
                    Mutability::Mutable => Type::Ptr(Box::new(converted)),
                    Mutability::Immutable => Type::ConstPtr(Box::new(converted)),
                })
            }
            &Ty::Path(_, ref p) => {
                match p.convert_to_simple_single_segment() {
                    Some(p) => {
                        if let Some(prim) = convert_path_name_to_primitive(&p) {
                            Ok(Type::Primitive(prim))
                        } else {
                            Ok(Type::Path(p))
                        }
                    }
                    None => Err(format!("not a simple single segment")),
                }
            }
            &Ty::Array(ref ty, ConstExpr::Lit(Lit::Int(sz, _))) => {
                let converted = try!(Type::convert(ty));

                Ok(Type::Array(Box::new(converted), sz))
            },
            &Ty::BareFn(ref f) => {
                let args = f.inputs.iter()
                                   .map(|x| Type::convert(&x.ty).ok())
                                   .collect::<Vec<_>>();
                let ret = try!(f.output.as_type());

                if args.iter().all(|x| x.is_some()) {
                    Ok(Type::FuncPtr(
                        ret.map(|x| Box::new(x)),
                        args.iter().filter_map(|x| x.clone()).collect(),
                    ))
                } else {
                    Err(format!("one of the params failed to convert"))
                }
            }
            _ => Err(format!("unexpected type")),
        }
    }

    pub fn add_deps(&self, library: &Library, out: &mut Vec<PathValue>) {
        match self {
            &Type::ConstPtr(ref t) => {
                t.add_deps(library, out);
            }
            &Type::Ptr(ref t) => {
                t.add_deps(library, out);
            }
            &Type::Path(ref p) => {
                library.add_deps_for_path(p, out);
            }
            &Type::Primitive(_) => { }
            &Type::Array(ref t, _) => {
                t.add_deps(library, out);
            }
            &Type::FuncPtr(ref opt_ret, ref args) => {
                if let Some(ref ty) = opt_ret.as_ref() {
                    ty.add_deps(library, out);
                }
                for arg in args {
                    arg.add_deps(library, out);
                }
            }
        }
    }

    pub fn specialize(&self, mappings: &Vec<(&String, &String)>) -> Type {
        match self {
            &Type::ConstPtr(ref t) => {
                Type::ConstPtr(Box::new(t.specialize(mappings)))
            }
            &Type::Ptr(ref t) => {
                Type::Ptr(Box::new(t.specialize(mappings)))
            }
            &Type::Path(ref p) => {
                let mut p = p.clone();

                for &(param, value) in mappings {
                    if p == *param {
                        p = value.clone();
                        break;
                    }
                }

                Type::Path(p)
            }
            &Type::Primitive(ref p) => {
                Type::Primitive(p.clone())
            }
            &Type::Array(ref t, ref sz) => {
                Type::Array(Box::new(t.specialize(mappings)), *sz)
            }
            &Type::FuncPtr(ref opt_ret, ref args) => {
                Type::FuncPtr(opt_ret.as_ref().map(|x| Box::new(x.specialize(mappings))),
                              args.iter()
                                  .map(|x| x.specialize(mappings))
                                  .collect())
            }
        }
    }

    fn write<F: Write>(&self, out: &mut F) {
        match self {
            &Type::ConstPtr(ref t) => {
                write!(out, "const ").unwrap();
                t.write(out);
                write!(out, "*").unwrap();
            }
            &Type::Ptr(ref t) => {
                t.write(out);
                write!(out, "*").unwrap();
            }
            &Type::Path(ref p) => {
                write!(out, "{}", p).unwrap();
            }
            &Type::Primitive(ref p) => {
                write!(out, "{}", p).unwrap();
            }
            &Type::Array(ref t, ref sz) => {
                t.write(out);
                write!(out, "[{}]", sz).unwrap();
            }
            &Type::FuncPtr(ref opt_ret, ref args) => {
                if let &Some(ref ret) = opt_ret {
                    ret.write(out);
                } else {
                    write!(out, "void").unwrap();
                }
                write!(out, " (*)(").unwrap();
                for (i, arg) in args.iter().enumerate() {
                    if i != 0 {
                        write!(out, ", ").unwrap();
                    }
                    arg.write(out);
                }
                write!(out, ")").unwrap();
            }
        }
    }

    fn write_with_ident<F: Write>(&self, ident: &str, out: &mut F) {
        match self {
            &Type::ConstPtr(ref t) => {
                write!(out, "const ").unwrap();
                t.write(out);
                write!(out, "* {}", ident).unwrap();

            }
            &Type::Ptr(ref t) => {
                t.write(out);
                write!(out, "* {}", ident).unwrap();
            }
            &Type::Path(ref p) => {
                write!(out, "{} {}", p, ident).unwrap();
            }
            &Type::Primitive(ref p) => {
                write!(out, "{} {}", p, ident).unwrap();
            }
            &Type::Array(ref t, ref sz) => {
                t.write(out);
                write!(out, " {}[{}]", ident, sz).unwrap();
            }
            &Type::FuncPtr(ref opt_ret, ref args) => {
                if let &Some(ref ret) = opt_ret {
                    ret.write(out);
                } else {
                    write!(out, "void").unwrap();
                }
                write!(out, " (*{})(", ident).unwrap();
                for (i, arg) in args.iter().enumerate() {
                    if i != 0 {
                        write!(out, ", ").unwrap();
                    }
                    arg.write(out);
                }
                write!(out, ")").unwrap();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub wr_destructor_safe: bool,
    pub return_ty: Option<Type>,
    pub args: Vec<(String, Type)>,
}

impl Function {
    pub fn convert(name: String, wr_destructor_safe: bool, decl: &FnDecl) -> ConvertResult<Function> {
        let args = decl.inputs.iter()
                              .map(|x| x.as_ident_and_type().ok())
                              .collect::<Vec<_>>();
        let ret = try!(decl.output.as_type());

        if args.iter().all(|x| x.is_some()) {
            Ok(Function {
                name: name,
                wr_destructor_safe: wr_destructor_safe,
                return_ty: ret,
                args: args.iter().filter_map(|x| x.clone()).collect(),
            })
        } else {
            Err(format!("one of the params failed to convert"))
        }
    }

    pub fn add_deps(&self, library: &Library, out: &mut Vec<PathValue>) {
        if let &Some(ref ty) = &self.return_ty {
            ty.add_deps(library, out);
        }
        for &(_, ref arg) in &self.args {
            arg.add_deps(library, out);
        }
    }

    pub fn write<F: Write>(&self, config: &Config, out: &mut F) {
        if let Some(ref f) = config.function_prefix {
            write!(out, "{} ", f).unwrap();
        }

        match self.return_ty.as_ref() {
            Some(ty) => ty.write(out),
            None => write!(out, "void").unwrap(),
        }

        write!(out, "\n{}(", self.name).unwrap();
        for (i, arg) in self.args.iter().enumerate() {
            if i != 0 {
                write!(out, ",\n    ").unwrap();
            }
            arg.1.write_with_ident(&arg.0, out);
        }
        write!(out, ")").unwrap();

        if self.wr_destructor_safe {
            write!(out, "\nWR_DESTRUCTOR_SAFE_FUNC").unwrap()
        } else {
            if let Some(ref f) = config.function_prefix {
                write!(out, "\n{}", f).unwrap();
            }
        }
        write!(out, ";").unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    pub generic_params: Vec<PathRef>,
}

impl Struct {
    pub fn convert(name: String, decl: &VariantData, generics: &Generics) -> ConvertResult<Struct> {
        let fields = match decl {
            &VariantData::Struct(ref fields) => {
                fields.iter()
                      .map(|x| x.as_ident_and_type().ok())
                      .collect::<Vec<_>>()
            }
            &VariantData::Tuple(ref fields) => {
                let mut out = Vec::new();
                let mut current = 0;
                for field in fields {
                    out.push(Some((format!("m{}", current),
                                   try!(Type::convert(&field.ty)))));
                    current += 1;
                }
                out
            }
            &VariantData::Unit => {
                vec![]
            }
        };

        let generic_params = generics.ty_params.iter()
                                               .map(|x| x.ident.to_string())
                                               .collect::<Vec<_>>();

        if fields.iter().all(|x| x.is_some()) {
            Ok(Struct {
                name: name,
                fields: fields.iter().filter_map(|x| x.clone()).collect(),
                generic_params: generic_params,
            })
        } else {
            Err(format!("one of the fields failed to convert"))
        }
    }

    pub fn add_deps(&self, library: &Library, out: &mut Vec<PathValue>) {
        for &(_, ref ty) in &self.fields {
            ty.add_deps(library, out);
        }
    }

    pub fn write<F: Write>(&self, out: &mut F) {
        writeln!(out, "struct {} {{", self.name).unwrap();
        for (i, field) in self.fields.iter().enumerate() {
            if i != 0 {
                write!(out, "\n").unwrap();
            }
            write!(out, "  ").unwrap();
            field.1.write_with_ident(&field.0, out);
            write!(out, ";").unwrap();
        }
        write!(out, "\n\n").unwrap();

        writeln!(out, "  bool operator==(const {}& aOther) const {{", self.name).unwrap();
        write!(out, "    return ").unwrap();
        for (i, field) in self.fields.iter().enumerate() {
            if i != 0 {
                write!(out, " &&\n      ").unwrap();
            }
            write!(out, "{} == aOther.{}", field.0, field.0).unwrap();
        }
        writeln!(out, ";\n  }}").unwrap();

        write!(out, "}};").unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct OpaqueStruct {
    pub name: PathRef,
}

impl OpaqueStruct {
    pub fn new(name: String) -> OpaqueStruct {
        OpaqueStruct {
            name: name,
        }
    }

    pub fn write<F: Write>(&self, out: &mut F) {
        write!(out, "struct {};", self.name).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub values: Vec<(String, u64)>,
}

impl Enum {
    pub fn convert(name: String, variants: &Vec<Variant>) -> ConvertResult<Enum> {
        let mut values = Vec::new();
        let mut current = 0;

        for variant in variants {
            match variant.data {
                VariantData::Unit => {
                    match variant.discriminant {
                        Some(ConstExpr::Lit(Lit::Int(i, _))) => {
                            current = i;
                        }
                        Some(_) => {
                            return Err(format!("unsupported discriminant"));
                        }
                        None => { /* okay, we just use current */ }
                    }

                    values.push((variant.ident.to_string(), current));
                    current = current + 1;
                }
                _ => {
                    return Err(format!("unsupported variant"));
                }
            }
        }

        Ok(Enum {
            name: name,
            values: values,
        })
    }

    pub fn write<F: Write>(&self, config: &Config, out: &mut F) {
        writeln!(out, "enum class {} : uint32_t {{", self.name).unwrap();
        for (i, value) in self.values.iter().enumerate() {
            if i != 0 {
                write!(out, "\n").unwrap();
            }
            write!(out, "  {} = {},", value.0, value.1).unwrap();
        }
        if config.enum_add_sentinel {
            write!(out, "\n\n  Sentinel /* this must be last for serialization purposes. */").unwrap();
        }
        write!(out, "\n}};").unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct Specialization {
    pub name: String,
    pub aliased: PathRef,
    pub generic_values: Vec<PathRef>,
}

impl Specialization {
    pub fn convert(name: String, ty: &Ty) -> ConvertResult<Specialization> {
        match ty {
            &Ty::Path(ref _q, ref p) => {
                let (path, generics) = try!(p.convert_to_generic_single_segment().ok_or("not a generic single segment"));

                if path_name_is_primitive(&path) {
                    return Err(format!("can't specialize a primitive"));
                }

                Ok(Specialization {
                    name: name,
                    aliased: path,
                    generic_values: generics.iter()
                                            .map(|x| map_path_name_to_primitive(x))
                                            .collect(),
                })
            }
            _ => {
                Err(format!("not a path"))
            }
        }
    }

    pub fn add_deps(&self, library: &Library, out: &mut Vec<PathValue>) {
        if !path_name_is_primitive(&self.aliased) {
            library.add_deps_for_path_deps(&self.aliased, out);
        }
        for value in &self.generic_values {
            if !path_name_is_primitive(value) {
                library.add_deps_for_path(value, out);
            }
        }
    }

    pub fn specialize(&self, library: &Library) -> ConvertResult<PathValue> {
        match library.resolve_path(&self.aliased) {
            Some(aliased) => {
                match aliased {
                    PathValue::OpaqueStruct(_) => {
                        Ok(PathValue::OpaqueStruct(OpaqueStruct {
                            name: self.name.clone(),
                        }))
                    }
                    PathValue::Struct(aliased) => {
                        if self.generic_values.len() !=
                           aliased.generic_params.len() {
                            return Err(format!("incomplete specialization"));
                        }

                        let mappings = aliased.generic_params.iter()
                                                             .zip(self.generic_values.iter())
                                                             .collect::<Vec<_>>();
                        Ok(PathValue::Struct(Struct {
                            name: self.name.clone(),
                            fields: aliased.fields.iter()
                                                  .map(|x| (x.0.clone(), x.1.specialize(&mappings)))
                                                  .collect(),
                            generic_params: vec![],
                        }))
                    }
                    PathValue::Enum(aliased) => {
                        Ok(PathValue::Enum(Enum {
                            name: self.name.clone(),
                            values: aliased.values.clone(),
                        }))
                    }
                    PathValue::Prebuilt(aliased) => {
                        Ok(PathValue::Prebuilt(Prebuilt {
                            // assume that the prebuilt has the right name
                            name: aliased.name,
                            source: aliased.source,
                        }))
                    }
                    _ => Err(format!("unknown type to specialize"))
                }
            }
            None => {
                Err(format!("couldn't find aliased type"))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Typedef {
    pub name: String,
    pub aliased: Type,
}

impl Typedef {
    pub fn convert(name: String, ty: &Ty) -> ConvertResult<Typedef> {
        Ok(Typedef {
            name: name,
            aliased: try!(Type::convert(ty)),
        })
    }

    pub fn add_deps(&self, library: &Library, out: &mut Vec<PathValue>) {
        self.aliased.add_deps(library, out);
    }

    pub fn write<F: Write>(&self, out: &mut F) {
        write!(out, "typedef ").unwrap();
        self.aliased.write_with_ident(&self.name, out);
        write!(out, ";").unwrap();
    }
}