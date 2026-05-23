//! Proc-macro derives for the [`nlink`][nlink] Linux-netlink
//! library — typed GENL family / command / attribute codecs.
//!
//! See the [Plan 154][plan-154] for the design + roadmap and
//! `crates/nlink/examples/macros/` (when shipped) for end-to-end
//! examples of defining a new GENL family in ~20 lines of
//! declarative code.
//!
//! # Shipped derives (0.16 Phase 1)
//!
//! - [`macro@GenlCommand`] + `#[genl_command(repr = "u8"|"u16")]`
//!   — typed GENL command enum
//!
//! The remaining derives ([`GenlAttribute`][note-1],
//! [`GenlEnum`][note-1], [`GenlMessage`][note-1],
//! [`NetlinkAttrs`][note-1], and the `#[genl_family]` attribute
//! macro) ship in subsequent phases of Plan 154.
//!
//! [nlink]: https://docs.rs/nlink
//! [plan-154]: https://github.com/p13marc/nlink/blob/master/plans/154-0.16-nlink-macros-plan.md
//! [note-1]: # "follow-up phases of Plan 154 — not yet shipped"

use proc_macro::TokenStream;
use proc_macro2::Span;
use syn::{parse_macro_input, Data, DeriveInput, Expr, ExprLit, Lit, LitStr, Meta};

mod genl_command;

/// Derive a typed-enum codec for a Generic Netlink **command** ID
/// enum.
///
/// Generates `impl From<EnumType> for ReprType` (infallible —
/// every variant has a known discriminant) and `impl
/// TryFrom<ReprType> for EnumType` (fallible — unknown values
/// land in an `Err(InvalidValue)` arm).
///
/// `ReprType` is either `u8` or `u16` per the
/// `#[genl_command(repr = "...")]` attribute.
///
/// # Example
///
/// ```ignore
/// use nlink_macros::GenlCommand;
///
/// #[derive(GenlCommand, Debug, Clone, Copy, PartialEq, Eq)]
/// #[genl_command(repr = "u8")]
/// #[non_exhaustive]
/// pub enum MyCmd {
///     Unspec = 0,
///     Get = 1,
///     Set = 2,
/// }
///
/// // Generated impls:
/// let raw: u8 = MyCmd::Get.into();
/// assert_eq!(raw, 1);
/// let parsed = MyCmd::try_from(2u8).unwrap();
/// assert_eq!(parsed, MyCmd::Set);
/// assert!(MyCmd::try_from(255u8).is_err());
/// ```
///
/// # Requirements
///
/// - The annotated type must be an `enum`.
/// - Each variant must have an explicit `= literal` discriminant
///   (e.g. `Get = 1`). Anonymous-discriminant variants (`Get,`)
///   are rejected at compile time because kernel ABI demands
///   stable wire values.
/// - Variants must be unit-only (no fields). Tuple/struct
///   variants are rejected.
///
/// Errors point at the offending span via `syn::Error::new_spanned`.
#[proc_macro_derive(GenlCommand, attributes(genl_command))]
pub fn derive_genl_command(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    genl_command::expand(input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

// --------------------------------------------------------------
// Shared helpers used across derives. Kept in lib.rs because
// they're small + private to this crate.
// --------------------------------------------------------------

/// Width of a typed-codec enum's wire representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReprWidth {
    U8,
    U16,
    U32,
}

impl ReprWidth {
    pub(crate) fn ident(self) -> proc_macro2::Ident {
        let s = match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
        };
        proc_macro2::Ident::new(s, Span::call_site())
    }

    /// Parse from the `repr = "u8"` string-literal form used by
    /// `#[genl_command(repr = "...")]` and siblings.
    pub(crate) fn parse(lit: &LitStr) -> syn::Result<Self> {
        match lit.value().as_str() {
            "u8" => Ok(Self::U8),
            "u16" => Ok(Self::U16),
            "u32" => Ok(Self::U32),
            other => Err(syn::Error::new(
                lit.span(),
                format!("unknown repr {other:?}; expected \"u8\", \"u16\", or \"u32\""),
            )),
        }
    }
}

/// Find the `#[genl_command(...)]` (or other named) attribute on
/// the derive input and return its `Meta::List`.
pub(crate) fn find_meta_list<'a>(
    attrs: &'a [syn::Attribute],
    name: &str,
) -> Option<&'a syn::MetaList> {
    attrs.iter().find_map(|a| match &a.meta {
        Meta::List(ml) if ml.path.is_ident(name) => Some(ml),
        _ => None,
    })
}

/// Parse `repr = "u8"` (etc.) out of the `Meta::List` inside an
/// attribute. Returns `Err` if `repr` is missing or malformed.
pub(crate) fn parse_repr_attr(ml: &syn::MetaList, attr_name: &str) -> syn::Result<ReprWidth> {
    let mut found_repr: Option<ReprWidth> = None;
    ml.parse_nested_meta(|meta| {
        if meta.path.is_ident("repr") {
            let value = meta.value()?;
            let lit: LitStr = value.parse()?;
            found_repr = Some(ReprWidth::parse(&lit)?);
            Ok(())
        } else {
            Err(meta.error(format!(
                "unknown {attr_name} key {:?}; expected `repr`",
                meta.path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default()
            )))
        }
    })?;
    found_repr.ok_or_else(|| {
        syn::Error::new_spanned(
            ml,
            format!("#[{attr_name}(...)] must specify `repr = \"u8\"|\"u16\"|\"u32\"`"),
        )
    })
}

/// Extract the explicit `= literal` discriminant from a variant.
/// Returns the literal value as a `u64` (variants must fit; the
/// derive validates against the repr's width separately).
pub(crate) fn variant_discriminant(variant: &syn::Variant) -> syn::Result<u64> {
    let (_, expr) = variant.discriminant.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(
            variant,
            "GenlCommand/GenlAttribute/GenlEnum variants must have an \
             explicit `= literal` discriminant — kernel ABI requires \
             stable wire values",
        )
    })?;
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse::<u64>(),
        _ => Err(syn::Error::new_spanned(
            expr,
            "discriminant must be an integer literal (e.g., `= 1`)",
        )),
    }
}

/// Ensure the data is an enum + every variant is a unit variant
/// (no fields).
pub(crate) fn require_unit_enum<'a>(
    data: &'a Data,
    derive_name: &str,
    span: Span,
) -> syn::Result<&'a syn::DataEnum> {
    let de = match data {
        Data::Enum(e) => e,
        Data::Struct(_) => {
            return Err(syn::Error::new(
                span,
                format!("#[derive({derive_name})] is only valid on enums, not structs"),
            ))
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                span,
                format!("#[derive({derive_name})] is only valid on enums, not unions"),
            ))
        }
    };
    for v in &de.variants {
        match &v.fields {
            syn::Fields::Unit => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    v,
                    format!(
                        "#[derive({derive_name})] variants must be unit-only \
                         (no fields); `{}` has fields",
                        v.ident
                    ),
                ))
            }
        }
    }
    Ok(de)
}

/// Validate that `value` fits in `width` (the discriminant doesn't
/// overflow the chosen repr).
pub(crate) fn fits_in_width(value: u64, width: ReprWidth) -> bool {
    match width {
        ReprWidth::U8 => value <= u8::MAX as u64,
        ReprWidth::U16 => value <= u16::MAX as u64,
        ReprWidth::U32 => value <= u32::MAX as u64,
    }
}

