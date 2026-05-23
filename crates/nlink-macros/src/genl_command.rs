//! `#[derive(GenlCommand)]` expansion.
//!
//! Generates `From<EnumType> for ReprType` (infallible) and
//! `TryFrom<ReprType> for EnumType` (returns `Err(())` for
//! unknown values). The repr width is taken from
//! `#[genl_command(repr = "u8"|"u16")]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

use crate::{
    find_meta_list, fits_in_width, parse_repr_attr, require_unit_enum, variant_discriminant,
    ReprWidth,
};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let enum_ident = &input.ident;
    let span = enum_ident.span();

    // Parse #[genl_command(repr = "u8")] attribute.
    let ml = find_meta_list(&input.attrs, "genl_command").ok_or_else(|| {
        syn::Error::new(
            span,
            "#[derive(GenlCommand)] requires #[genl_command(repr = \"u8\"|\"u16\")] \
             attribute",
        )
    })?;
    let width = parse_repr_attr(ml, "genl_command")?;
    // GenlCommand is u8-or-u16 only (commands are by convention
    // narrow); reject u32 with a clear error.
    if matches!(width, ReprWidth::U32) {
        return Err(syn::Error::new_spanned(
            ml,
            "#[genl_command(repr = \"u32\")] is not allowed — GENL commands are \
             u8-or-u16 by kernel convention; use #[derive(GenlEnum)] for \
             u32-wide value enums (e.g. policy/mode codes)",
        ));
    }

    // Validate it's a unit-variant enum + extract (Ident, value)
    // pairs.
    let de = require_unit_enum(&input.data, "GenlCommand", span)?;
    if de.variants.is_empty() {
        return Err(syn::Error::new(
            span,
            "#[derive(GenlCommand)] requires at least one variant",
        ));
    }
    let mut variants = Vec::with_capacity(de.variants.len());
    for v in &de.variants {
        let value = variant_discriminant(v)?;
        if !fits_in_width(value, width) {
            return Err(syn::Error::new_spanned(
                v,
                format!(
                    "variant discriminant {value} overflows the chosen repr ({})",
                    width.ident()
                ),
            ));
        }
        variants.push((v.ident.clone(), value));
    }

    let repr = width.ident();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // From<EnumType> for ReprType — infallible.
    let from_arms = variants.iter().map(|(ident, value)| {
        let lit = proc_macro2::Literal::u64_unsuffixed(*value);
        quote! { #enum_ident::#ident => #lit }
    });

    // TryFrom<ReprType> for EnumType — error on unknown values.
    let tryfrom_arms = variants.iter().map(|(ident, value)| {
        let lit = proc_macro2::Literal::u64_unsuffixed(*value);
        quote! { #lit => ::core::result::Result::Ok(#enum_ident::#ident) }
    });

    // The TryFrom error is the unknown wire value, wrapped in a
    // tiny zero-overhead type so callers can match on it without
    // pulling in a separate error enum from the runtime crate.
    let error_ident =
        proc_macro2::Ident::new(&format!("{enum_ident}UnknownValue"), enum_ident.span());

    Ok(quote! {
        /// Error returned by the generated
        /// `TryFrom<wire repr>` impl when the wire value
        /// doesn't match any declared variant.
        ///
        /// Carries the original raw value so callers can log or
        /// propagate it through their own error type.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct #error_ident(pub #repr);

        impl ::core::fmt::Display for #error_ident {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(
                    f,
                    "unknown {} value: {}",
                    ::core::stringify!(#enum_ident),
                    self.0
                )
            }
        }

        impl ::core::error::Error for #error_ident {}

        impl #impl_generics ::core::convert::From<#enum_ident #ty_generics> for #repr #where_clause {
            #[inline]
            fn from(value: #enum_ident #ty_generics) -> Self {
                match value {
                    #(#from_arms,)*
                }
            }
        }

        impl #impl_generics ::core::convert::TryFrom<#repr> for #enum_ident #ty_generics #where_clause {
            type Error = #error_ident;

            #[inline]
            fn try_from(value: #repr) -> ::core::result::Result<Self, Self::Error> {
                match value {
                    #(#tryfrom_arms,)*
                    other => ::core::result::Result::Err(#error_ident(other)),
                }
            }
        }
    })
}
