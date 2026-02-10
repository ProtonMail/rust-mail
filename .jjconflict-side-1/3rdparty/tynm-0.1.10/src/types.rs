use alloc::{boxed::Box, string::String, vec::Vec};
use core::{
    fmt,
    fmt::{Error, Write},
};

use crate::{parser, TypeParamsFmtOpts};

/// Helper struct for printing type names directly to `format!`.
///
/// This struct warps `TypeName` and implements `fmd::Display` to serve as
/// `{}` argument to `format!` and similar macros.
/// It can be obtained by the `as_display` and `as_display_mn` method.
///
/// # Example
///
/// ```rust
/// use tynm::TypeName;
///
/// let tn = TypeName::new::<usize>();
///
/// println!("{}", tn.as_display());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameDisplay<'s> {
    inner: &'s TypeName<'s>,
    segment_count_left: usize,
    segment_count_right: usize,
    type_params_fmt_opts: TypeParamsFmtOpts,
}

impl<'s> fmt::Display for TypeNameDisplay<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.write_str_opts(
            f,
            self.segment_count_left,
            self.segment_count_right,
            self.type_params_fmt_opts,
        )
    }
}

/// Organizes type name string into distinct parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeName<'s> {
    None,
    Array(TypeNameArray<'s>),
    // Function(TypeNameFunction<'s>), // TODO
    Never,
    Pointer(TypeNamePointer<'s>),
    Reference(TypeNameReference<'s>),
    Slice(TypeNameSlice<'s>),
    Struct(TypeNameStruct<'s>),
    Tuple(TypeNameTuple<'s>),
    Trait(TypeNameTrait<'s>),
    Unit,
}

impl<'s> TypeName<'s> {
    /// Constructs a new TypeName with the name of `T`.
    ///
    /// This is equivalent to calling `core::any::type_name::<T>().into()`
    pub fn new<T: ?Sized>() -> Self {
        core::any::type_name::<T>().into()
    }

    /// Returns the type name string without any module paths.
    ///
    /// This is equivalent to calling `TypeName::as_str_mn_opts(0, 0, TypeParamsFmtOpts::All);`
    pub fn as_str(&self) -> String {
        self.as_str_mn(0, 0)
    }

    /// Returns the type name string without any module paths.
    ///
    /// This is equivalent to calling `TypeName::as_str_mn_opts(0, 0, type_params_fmt_opts);`
    pub fn as_str_opts(&self, type_params_fmt_opts: TypeParamsFmtOpts) -> String {
        self.as_str_mn_opts(0, 0, type_params_fmt_opts)
    }

    /// Returns the type name string with the given number of module segments.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// printed once.
    ///
    /// This is equivalent to calling `TypeName::as_str_mn_opts(m, n, TypeParamsFmtOpts::All);`
    ///
    /// # Parameters
    ///
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn as_str_mn(&self, m: usize, n: usize) -> String {
        self.as_str_mn_opts(m, n, TypeParamsFmtOpts::All)
    }
    /// Returns the type name string with the given number of module segments.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// printed once.
    ///
    /// # Parameters
    ///
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn as_str_mn_opts(
        &self,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> String {
        let mut buffer = String::with_capacity(128); // TODO: smarter capacity allocation.

        self.write_str_opts(&mut buffer, m, n, type_params_fmt_opts)
            .unwrap_or_else(|e| panic!("Failed to write `TypeName` as String. Error: `{}`.", e));

        buffer
    }

    /// Returns an object that implements `fmt::Display` for printing the type
    /// name without any module paths directly with `format!` and `{}`.
    ///
    /// When using this type name in a `format!` or similar it is more efficient
    /// to use this display instead of first creating a string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tynm::TypeName;
    ///
    /// let tn = TypeName::new::<String>();
    ///
    /// println!("{}", tn.as_display());
    /// ```
    pub fn as_display(&self) -> TypeNameDisplay {
        self.as_display_opts(TypeParamsFmtOpts::All)
    }

    /// Returns an object that implements `fmt::Display` for printing the type
    /// name without any module paths directly with `format!` and `{}`.
    ///
    /// When using this type name in a `format!` or similar it is more efficient
    /// to use this display instead of first creating a string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tynm::{TypeName, TypeParamsFmtOpts};
    ///
    /// let tn = TypeName::new::<String>();
    ///
    /// println!("{}", tn.as_display_opts(TypeParamsFmtOpts::Std));
    /// ```
    pub fn as_display_opts(&self, type_params_fmt_opts: TypeParamsFmtOpts) -> TypeNameDisplay {
        TypeNameDisplay {
            inner: self,
            segment_count_left: 0,
            segment_count_right: 0,
            type_params_fmt_opts,
        }
    }

    /// Returns an object that implements `fmt::Display` for printing the type
    /// name without any module paths directly with `format!` and `{}`.
    ///
    /// When using this type name in a `format!` or similar it is more efficient
    /// to use this display instead of first creating a string.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    ///
    /// # Example
    ///
    /// ```rust
    /// use tynm::TypeName;
    ///
    /// let tn = TypeName::new::<String>();
    ///
    /// println!("{}", tn.as_display_mn(1, 2));
    /// ```
    pub fn as_display_mn(&self, m: usize, n: usize) -> TypeNameDisplay {
        self.as_display_mn_opts(m, n, TypeParamsFmtOpts::All)
    }

    /// Returns an object that implements `fmt::Display` for printing the type
    /// name without any module paths directly with `format!` and `{}`.
    ///
    /// When using this type name in a `format!` or similar it is more efficient
    /// to use this display instead of first creating a string.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tynm::{TypeName, TypeParamsFmtOpts};
    ///
    /// let tn = TypeName::new::<String>();
    ///
    /// println!("{}", tn.as_display_mn_opts(1, 2, TypeParamsFmtOpts::Std));
    /// ```
    pub fn as_display_mn_opts(
        &self,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> TypeNameDisplay {
        TypeNameDisplay {
            inner: self,
            segment_count_left: m,
            segment_count_right: n,
            type_params_fmt_opts,
        }
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn write_str<W>(&self, buffer: &mut W, m: usize, n: usize) -> Result<(), Error>
    where
        W: Write,
    {
        self.write_str_opts(buffer, m, n, TypeParamsFmtOpts::All)
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str_opts<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        match self {
            Self::None => Ok(()),
            Self::Array(type_name_array) => {
                type_name_array.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Never => buffer.write_str("!"),
            Self::Pointer(type_name_pointer) => {
                type_name_pointer.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Reference(type_name_reference) => {
                type_name_reference.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Slice(type_name_slice) => {
                type_name_slice.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Struct(type_name_struct) => {
                type_name_struct.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Tuple(type_name_tuple) => {
                type_name_tuple.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Trait(type_name_trait) => {
                type_name_trait.write_str(buffer, m, n, type_params_fmt_opts)
            }
            Self::Unit => buffer.write_str("()"),
        }
    }
}

/// Type name of an array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameArray<'s> {
    /// Type of each array element.
    pub(crate) type_param: Box<TypeName<'s>>,
    /// Array length.
    pub(crate) len: &'s str,
}

impl<'s> TypeNameArray<'s> {
    /// Returns the type parameter of this type.
    pub fn type_param(&self) -> &TypeName<'s> {
        &self.type_param
    }

    /// Returns the type parameter of this type.
    pub fn len(&self) -> &str {
        self.len
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        buffer.write_str("[")?;
        self.type_param
            .write_str_opts(buffer, m, n, type_params_fmt_opts)?;
        buffer.write_str("; ")?;
        buffer.write_str(self.len)?;
        buffer.write_str("]")
    }
}

/// Type name of a pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNamePointer<'s> {
    /// Type of pointer.
    pub(crate) const_or_mut: &'s str,
    /// Type pointed to.
    pub(crate) type_param: Box<TypeName<'s>>,
}

impl<'s> TypeNamePointer<'s> {
    /// Returns the `"const"` or `"mut"` str.
    pub fn const_or_mut(&self) -> &str {
        self.const_or_mut
    }

    /// Returns the type parameter of this type.
    pub fn type_param(&self) -> &TypeName<'s> {
        &self.type_param
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        buffer.write_str("* ")?;
        buffer.write_str(self.const_or_mut)?;
        buffer.write_str(" ")?;
        self.type_param
            .write_str_opts(buffer, m, n, type_params_fmt_opts)
    }
}

/// Type name of a reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameReference<'s> {
    /// Type of reference.
    pub(crate) mutable: bool,
    /// Type referenced.
    pub(crate) type_param: Box<TypeName<'s>>,
}

impl<'s> TypeNameReference<'s> {
    /// Returns whether the reference is mutable.
    pub fn mutable(&self) -> bool {
        self.mutable
    }

    /// Returns the type parameter of this type.
    pub fn type_param(&self) -> &TypeName<'s> {
        &self.type_param
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        buffer.write_str("&")?;
        if self.mutable {
            buffer.write_str("mut ")?;
        }
        self.type_param
            .write_str_opts(buffer, m, n, type_params_fmt_opts)
    }
}

/// Type name of a slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameSlice<'s> {
    /// Type of each slice element.
    pub(crate) type_param: Box<TypeName<'s>>,
}

impl<'s> TypeNameSlice<'s> {
    /// Returns the type parameter of this type.
    pub fn type_param(&self) -> &TypeName<'s> {
        &self.type_param
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        // Don't need to prepend with `"&"` because slices are always passed in as references.
        buffer.write_str("[")?;
        self.type_param
            .write_str_opts(buffer, m, n, type_params_fmt_opts)?;
        buffer.write_str("]")
    }
}

/// Type name of a struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameStruct<'s> {
    /// Module path of this type.
    pub(crate) module_path: Vec<&'s str>,
    /// Simple type name, excluding type parameters.
    pub(crate) simple_name: &'s str,
    /// Type parameters to this type.
    pub(crate) type_params: Vec<TypeName<'s>>,
}

impl<'s> TypeNameStruct<'s> {
    /// Returns the module path of the type.
    pub fn module_path(&self) -> &[&'s str] {
        &self.module_path
    }

    /// Returns the simple name of the type, excluding type parameters.
    pub fn simple_name(&self) -> &'s str {
        self.simple_name
    }

    /// Returns the type parameters of this type.
    pub fn type_params(&self) -> &[TypeName<'s>] {
        &self.type_params
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        let result = self
            .write_module_path(buffer, m, n)
            .and_then(|_| self.write_simple_name(buffer));

        match type_params_fmt_opts {
            TypeParamsFmtOpts::All => {
                result.and_then(|_| self.write_type_params(buffer, m, n, type_params_fmt_opts))
            }
            TypeParamsFmtOpts::Std
                if matches!(
                    self.module_path.first().copied(),
                    Some("std" | "core" | "alloc")
                ) =>
            {
                result.and_then(|_| self.write_type_params(buffer, m, n, type_params_fmt_opts))
            }
            _ => result,
        }
    }

    /// Writes the module path to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn write_module_path<W>(&self, buffer: &mut W, m: usize, n: usize) -> Result<(), Error>
    where
        W: Write,
    {
        let module_segment_count = m.saturating_add(n);

        if module_segment_count >= self.module_path.len() {
            // Print full module path
            buffer.write_str(&self.module_path.join("::"))?;
        } else {
            // Print leading and trailing module segments
            buffer.write_str(&self.module_path[0..m].join("::"))?;

            if m > 0 {
                buffer.write_str("::")?;
            }

            // If we skipped any module segments, indicate this with `".."`
            if module_segment_count > 0 {
                buffer.write_str("..")?;
            }

            if n > 0 {
                buffer.write_str("::")?;
            }

            let len = self.module_path.len();
            buffer.write_str(&self.module_path[(len - n)..len].join("::"))?;
        }

        if module_segment_count > 0 {
            buffer.write_str("::")?;
        }

        Ok(())
    }

    /// Writes the simple name to the given buffer.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    pub fn write_simple_name<W>(&self, buffer: &mut W) -> Result<(), Error>
    where
        W: Write,
    {
        buffer.write_str(self.simple_name)
    }

    /// Writes type parameters to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn write_type_params<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        if !self.type_params.is_empty() {
            buffer.write_str("<")?;

            if let Some((first, rest)) = self.type_params.split_first() {
                first.write_str_opts(buffer, m, n, type_params_fmt_opts)?;
                rest.iter().try_for_each(|type_param| {
                    buffer
                        .write_str(", ")
                        .and_then(|_| type_param.write_str_opts(buffer, m, n, type_params_fmt_opts))
                })?;
            }

            buffer.write_str(">")?;
        }

        Ok(())
    }
}

/// Type name of a tuple.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameTuple<'s> {
    /// Type parameters to this type.
    pub(crate) type_params: Vec<TypeName<'s>>,
}

impl<'s> TypeNameTuple<'s> {
    /// Returns the type parameters of this type.
    pub fn type_params(&self) -> &[TypeName<'s>] {
        &self.type_params
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        if !self.type_params.is_empty() {
            buffer.write_str("(")?;

            if let Some((first, rest)) = self.type_params.split_first() {
                first.write_str_opts(buffer, m, n, type_params_fmt_opts)?;

                if self.type_params.len() == 1 {
                    buffer.write_str(",")?; // Always write `,` after first type.
                } else {
                    rest.iter().try_for_each(|type_param| {
                        buffer.write_str(", ").and_then(|_| {
                            type_param.write_str_opts(buffer, m, n, type_params_fmt_opts)
                        })
                    })?;
                }
            }

            buffer.write_str(")")?;
        }

        Ok(())
    }
}

/// Type name of a trait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameTrait<'s> {
    /// Share implementation with [`TypeNameStruct`]
    pub(crate) inner: TypeNameStruct<'s>,
}

impl<'s> TypeNameTrait<'s> {
    /// Returns the module path of the type.
    pub fn module_path(&self) -> &[&'s str] {
        &self.inner.module_path
    }

    /// Returns the simple name of the type, excluding type parameters.
    pub fn simple_name(&self) -> &'s str {
        self.inner.simple_name
    }

    /// Returns the type parameters of this type.
    pub fn type_params(&self) -> &[TypeName<'s>] {
        &self.inner.type_params
    }

    /// Writes the type name string to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    /// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
    pub fn write_str<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        buffer.write_str("dyn ")?;
        self.inner.write_str(buffer, m, n, type_params_fmt_opts)
    }

    /// Writes the module path to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn write_module_path<W>(&self, buffer: &mut W, m: usize, n: usize) -> Result<(), Error>
    where
        W: Write,
    {
        self.inner.write_module_path(buffer, m, n)
    }

    /// Writes the simple name to the given buffer.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    pub fn write_simple_name<W>(&self, buffer: &mut W) -> Result<(), Error>
    where
        W: Write,
    {
        self.inner.write_simple_name(buffer)
    }

    /// Writes type parameters to the given buffer.
    ///
    /// If the left and right module segments overlap, the overlapping segments will only be printed
    /// once.
    ///
    /// # Parameters
    ///
    /// * `buffer`: Buffer to write to.
    /// * `m`: Number of module segments to include, beginning from the left (most significant).
    /// * `n`: Number of module segments to include, beginning from the right (least significant).
    pub fn write_type_params<W>(
        &self,
        buffer: &mut W,
        m: usize,
        n: usize,
        type_params_fmt_opts: TypeParamsFmtOpts,
    ) -> Result<(), Error>
    where
        W: Write,
    {
        self.inner
            .write_type_params(buffer, m, n, type_params_fmt_opts)
    }
}

impl<'s> From<&'s str> for TypeName<'s> {
    fn from(std_type_name: &'s str) -> Self {
        parser::type_name(std_type_name)
            .map(|(_input, type_name)| type_name)
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to parse `TypeName` for input string: `{}`. Error: `{:?}`.",
                    std_type_name, e,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use pretty_assertions::assert_eq;

    use super::{TypeName, TypeNameStruct};

    macro_rules! type_name_simple {
        () => {{
            TypeName::Struct(TypeNameStruct {
                module_path: vec!["tynm", "types", "tests"],
                simple_name: "Simple",
                type_params: Vec::new(),
            })
        }};
    }
    macro_rules! type_name_type_param_single {
        () => {{
            TypeName::Struct(TypeNameStruct {
                module_path: vec!["tynm", "types", "tests"],
                simple_name: "TypeParamSingle",
                type_params: vec![type_name_simple!()],
            })
        }};
    }

    #[test]
    fn parse_simple_struct() {
        let expected = type_name_simple!();

        let actual = TypeName::from(core::any::type_name::<Simple>());

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_type_parameterized_struct() {
        let expected = type_name_type_param_single!();

        let actual = TypeName::from(core::any::type_name::<TypeParamSingle<Simple>>());

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_nested_type_parameterized_struct() {
        let expected = TypeName::Struct(TypeNameStruct {
            module_path: vec!["tynm", "types", "tests"],
            simple_name: "TypeParamSingle",
            type_params: vec![type_name_type_param_single!()],
        });

        let actual = TypeName::from(core::any::type_name::<
            TypeParamSingle<TypeParamSingle<Simple>>,
        >());

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_multi_type_parameterized_struct() {
        let expected = TypeName::Struct(TypeNameStruct {
            module_path: vec!["tynm", "types", "tests"],
            simple_name: "TypeParamDouble",
            type_params: vec![type_name_simple!(), type_name_simple!()],
        });

        let actual = TypeName::from(core::any::type_name::<TypeParamDouble<Simple, Simple>>());

        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_nested_multi_type_parameterized_struct() {
        let expected = TypeName::Struct(TypeNameStruct {
            module_path: vec!["tynm", "types", "tests"],
            simple_name: "TypeParamDouble",
            type_params: vec![
                type_name_type_param_single!(),
                type_name_type_param_single!(),
            ],
        });

        let actual = TypeName::from(core::any::type_name::<
            TypeParamDouble<TypeParamSingle<Simple>, TypeParamSingle<Simple>>,
        >());

        assert_eq!(expected, actual);
    }

    struct Simple;
    struct TypeParamSingle<T>(T);
    struct TypeParamDouble<T, U>(T, U);
}
