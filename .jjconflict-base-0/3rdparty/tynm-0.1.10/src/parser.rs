use alloc::{boxed::Box, vec, vec::Vec};

use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while},
    character::complete::char,
    combinator::opt,
    multi::separated_list0,
    sequence::{delimited, pair, tuple},
    IResult,
};

use crate::types::{
    TypeName, TypeNameArray, TypeNameReference, TypeNameSlice, TypeNameStruct, TypeNameTrait,
    TypeNameTuple,
};

/// List of known primitive types
///
/// Note: Arrays and slices are not included in this list as their type name depends on the type
/// parameter (and length).
#[rustfmt::skip]
const PRIMITIVE_TYPES: &[&str] = &[
    // From side bar on <https://doc.rust-lang.org/std/primitive.slice.html>
    // "array", [T; n]
    "bool",
    "char",
    "f32",
    "f64",
    // "fn", // TODO
    "i128",
    "i16",
    "i32",
    "i64",
    "i8",
    "isize",
    // "never", !
    // "pointer", *
    // "reference", &
    // "slice", [T]
    "str",
    // "tuple", (T, ..)
    "u128",
    "u16",
    "u32",
    "u64",
    "u8",
    // "unit", ()
    "usize",
];

pub fn is_alphanumeric_underscore(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

pub fn is_lowercase_alphanumeric_underscore(c: char) -> bool {
    c == '_' || c.is_ascii_digit() || c.is_ascii_lowercase()
}

pub fn module_name(input: &str) -> IResult<&str, &str> {
    if let Some(first_char) = input.chars().next() {
        if first_char.is_ascii_alphabetic() && first_char.is_ascii_lowercase() {
            take_while(is_lowercase_alphanumeric_underscore)(input)
        } else {
            Ok((input, ""))
        }
    } else {
        Ok((input, ""))
    }
}

pub fn module_path(input: &str) -> IResult<&str, Vec<&str>> {
    separated_list0(tag("::"), module_name)(input).map(|(input, mut module_names)| {
        module_names.retain(|module_name| !module_name.is_empty());
        (input, module_names)
    })
}

pub fn type_simple_name(input: &str) -> IResult<&str, &str> {
    if let Some(first_char) = input.chars().next() {
        if first_char.is_ascii_alphabetic() && first_char.is_ascii_uppercase() {
            take_while(is_alphanumeric_underscore)(input)
        } else {
            Ok((input, ""))
        }
    } else {
        Ok((input, ""))
    }
}

pub fn type_parameters(input: &str) -> IResult<&str, Vec<TypeName>> {
    opt(delimited(
        char('<'),
        separated_list0(tag(", "), type_name),
        char('>'),
    ))(input)
    .map(|(input, type_params)| (input, type_params.unwrap_or_default()))
}

pub fn array_length(input: &str) -> IResult<&str, Option<&str>> {
    pair(tag("; "), take_until("]"))(input)
        .map(|(input, (_, len))| (input, Some(len)))
        .or_else(|e| {
            if let nom::Err::Error(nom::error::Error { input, code: _ }) = e {
                Ok((input, None))
            } else {
                Err(e)
            }
        })
}

pub fn array_or_slice_internal(input: &str) -> IResult<&str, TypeName> {
    tuple((type_name, array_length))(input).map(|(input, (type_param, len))| {
        let type_param = Box::new(type_param);
        if let Some(len) = len {
            (input, TypeName::Array(TypeNameArray { type_param, len }))
        } else {
            (input, TypeName::Slice(TypeNameSlice { type_param }))
        }
    })
}

pub fn array_or_slice(input: &str) -> IResult<&str, TypeName> {
    delimited(char('['), array_or_slice_internal, char(']'))(input)
}

pub fn parse_reference(input: &str) -> IResult<&str, TypeName> {
    tuple((char('&'), opt(tag("mut")), opt(char(' ')), type_name))(input).map(
        |(input, (_, mut_str, _, type_param))| {
            let type_param = Box::new(type_param);
            (
                input,
                TypeName::Reference(TypeNameReference {
                    mutable: mut_str.is_some(),
                    type_param,
                }),
            )
        },
    )
}

pub fn parse_unit(input: &str) -> IResult<&str, TypeName> {
    tag("()")(input).map(|(input, _)| (input, TypeName::Unit))
}

pub fn parse_tuple(input: &str) -> IResult<&str, TypeName> {
    delimited(
        char('('),
        separated_list0(tag(", "), type_name),
        tuple((opt(char(',')), char(')'))),
    )(input)
    .map(|(input, type_params)| {
        let type_name_tuple = TypeName::Tuple(TypeNameTuple { type_params });
        (input, type_name_tuple)
    })
}

pub fn parse_unit_or_tuple(input: &str) -> IResult<&str, TypeName> {
    alt((parse_unit, parse_tuple))(input)
}

pub fn named_primitive(input: &str) -> Option<IResult<&str, TypeName>> {
    if let Some(prim_type) = PRIMITIVE_TYPES
        .iter()
        .find(|prim_type| input.starts_with(*prim_type))
    {
        let remainder = &input[prim_type.len()..];
        let next_char = remainder.chars().next();
        let mut is_primitive_type = false;
        if next_char.is_none() {
            is_primitive_type = true;
        } else if let Some(c) = next_char {
            is_primitive_type = !is_lowercase_alphanumeric_underscore(c);
        }

        if is_primitive_type {
            // Treat this as a type.
            Some(Ok((
                remainder,
                TypeName::Struct(TypeNameStruct {
                    module_path: vec![],
                    simple_name: prim_type,
                    type_params: vec![],
                }),
            )))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn struct_type(input: &str) -> IResult<&str, TypeNameStruct> {
    // Parse this as a module name
    tuple((module_path, type_simple_name, type_parameters))(input).map(
        |(s, (module_path, simple_name, type_params))| {
            (
                s,
                TypeNameStruct {
                    module_path,
                    simple_name,
                    type_params,
                },
            )
        },
    )
}

pub fn named_primitive_or_struct(input: &str) -> IResult<&str, TypeName> {
    // Check for primitive types first, otherwise we cannot distinguish the `std::u32` module from
    // `u32` (type).
    //
    // We shouldn't have to worry about `use crate::u32; u32::SomeType` as the type name input
    // should be the fully qualified crate name as determined by Rust.
    if let Some(result) = named_primitive(input) {
        result
    } else {
        struct_type(input)
            .map(|(input, type_name_struct)| (input, TypeName::Struct(type_name_struct)))
    }
}

pub fn trait_type(input: &str) -> IResult<&str, TypeName> {
    struct_type(input).map(|(input, type_name_struct)| {
        (
            input,
            TypeName::Trait(TypeNameTrait {
                inner: type_name_struct,
            }),
        )
    })
}

/// Parses a type name.
pub fn type_name(input: &str) -> IResult<&str, TypeName> {
    // Primitive types begin with lowercase letters, but we have to detect them at this level, as
    // lower parsers (`module_name`, `type_simple_name`) cannot tell if `"std::"` precedes the input
    // it is given.
    //
    // In addition, types may begin with symbols, and we should detect them here and branch to the
    // relevant parsing functions.
    let mut chars = input.chars();
    if let Some(first_char) = chars.next() {
        match first_char {
            '[' => array_or_slice(input),
            '*' => unimplemented!("`tynm` is not implemented for pointer types."),
            '!' => nom::character::complete::char('!')(input)
                .map(|(input, _)| (input, TypeName::Never)),
            '&' => parse_reference(input),
            '(' => parse_unit_or_tuple(input),
            'd' => {
                let mut split = input.splitn(2, ' ');
                if let Some("dyn") = split.next() {
                    if let Some(remainder) = split.next() {
                        trait_type(remainder)
                    } else {
                        // We only have "dyn" as a token. User may have specified r#dyn as a struct name or function
                        // name, but this is unusual. For now, we treat it as a struct.
                        Ok((
                            "",
                            TypeName::Struct(TypeNameStruct {
                                module_path: vec![],
                                simple_name: "dyn",
                                type_params: vec![],
                            }),
                        ))
                    }
                } else {
                    named_primitive_or_struct(input)
                }
            }
            _ => named_primitive_or_struct(input),
        }
    } else {
        Ok((input, TypeName::None))
    }
}
