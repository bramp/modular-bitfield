use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use syn::spanned::Spanned as _;

pub fn generate(input: TokenStream2) -> TokenStream2 {
    match generate_or_error(input) {
        Ok(output) => output,
        Err(err) => err.to_compile_error(),
    }
}

fn generate_or_error(input: TokenStream2) -> syn::Result<TokenStream2> {
    let input = syn::parse::<syn::DeriveInput>(input.into())?;
    match input.data {
        syn::Data::Enum(data_enum) => {
            generate_enum(syn::ItemEnum {
                attrs: input.attrs,
                vis: input.vis,
                enum_token: data_enum.enum_token,
                ident: input.ident,
                generics: input.generics,
                brace_token: data_enum.brace_token,
                variants: data_enum.variants,
            })
        }
        syn::Data::Struct(_) => {
            Err(format_err!(
                input,
                "structs are not supported as bitfield specifiers",
            ))
        }
        syn::Data::Union(_) => {
            Err(format_err!(
                input,
                "unions are not supported as bitfield specifiers",
            ))
        }
    }
}
struct Attributes {
    bits: Option<usize>,
    endian: Option<usize>, // TODO switch to enum
}

fn parse_attrs(attrs: &[syn::Attribute]) -> syn::Result<Attributes> {
    let mut attributes = Attributes { bits: None, endian: None };

    for attr in attrs {
        if attr.path.is_ident("bits") {
            if attributes.bits.is_some() {
                return Err(format_err_spanned!(
                    attr,
                    "More than one 'bits' attributes is not permitted",
                ))
            }

            let meta = attr.parse_meta()?;
            attributes.bits = match meta {
                syn::Meta::NameValue(syn::MetaNameValue {
                    lit: syn::Lit::Int(lit),
                    ..
                }) => Some(lit.base10_parse::<usize>()?),
                _ => {
                    return Err(format_err_spanned!(
                        attr,
                        "could not parse 'bits' attribute",
                    ))
                }
            };
        }

        if attr.path.is_ident("endian") {
            if attributes.endian.is_some() {
                return Err(format_err_spanned!(
                    attr,
                    "More than one 'endian' attributes is not permitted",
                ))
            }

            let meta = attr.parse_meta()?;
            attributes.endian = match meta {
                syn::Meta::NameValue(syn::MetaNameValue {
                    lit: syn::Lit::Int(lit),
                    ..
                }) => Some(lit.base10_parse::<usize>()?),
                _ => {
                    return Err(format_err_spanned!(
                        attr,
                        "could not parse 'endian' attribute",
                    ))
                }
            };
        }
    }

    Ok(attributes)
}

fn generate_enum(input: syn::ItemEnum) -> syn::Result<TokenStream2> {
    let span = input.span();
    let attributes = parse_attrs(&input.attrs)?;
    let enum_ident = &input.ident;

    let bits = match attributes.bits {
        Some(bits) => bits,
        None => {
            let count_variants = input.variants.iter().count();
            if !count_variants.is_power_of_two() {
                return Err(format_err!(
                    span,
                    "BitfieldSpecifier expected a number of variants which is a power of 2, specify #[bits = {}] if that was your intent",
                    count_variants.next_power_of_two().trailing_zeros(),
                ))
            }
            // We can take `trailing_zeros` returns type as the required amount of bits.
            match count_variants.checked_next_power_of_two() {
                Some(power_of_two) => power_of_two.trailing_zeros() as usize,
                None => {
                    return Err(format_err!(
                        span,
                        "BitfieldSpecifier has too many variants to pack into a bitfield",
                    ))
                }
            }
        }
    };

    let endian = match attributes.endian {
        Some(endian) => endian, // 1 big, 2 little
        None => 0, // Default to host endian
    };

    println!("{} endian {}", enum_ident, endian);

    let variants = input
        .variants
        .iter()
        .filter_map(|variant| {
            match &variant.fields {
                syn::Fields::Unit => Some(&variant.ident),
                _ => None,
            }
        })
        .collect::<Vec<_>>();

    let check_discriminants = variants.iter().map(|ident| {
        let span = ident.span();
        quote_spanned!(span =>
            impl ::modular_bitfield::private::checks::CheckDiscriminantInRange<[(); Self::#ident as usize]> for #enum_ident {
                type CheckType = [(); ((Self::#ident as usize) < (0x01_usize << #bits)) as usize ];
            }
        )
    });
    let from_bytes_arms = variants.iter().map(|ident| {
        let span = ident.span();
        quote_spanned!(span=>
            __bitfield_binding if __bitfield_binding == Self::#ident as <Self as ::modular_bitfield::Specifier>::Bytes => {
                ::core::result::Result::Ok(Self::#ident)
            }
        )
    });

    Ok(quote_spanned!(span=>
        #( #check_discriminants )*

        impl ::modular_bitfield::Specifier for #enum_ident {
            const BITS: usize = #bits;
            type Bytes = <[(); #bits] as ::modular_bitfield::private::SpecifierBytes>::Bytes;
            type InOut = Self;

            #[inline]
            fn into_bytes(input: Self::InOut) -> ::core::result::Result<Self::Bytes, ::modular_bitfield::error::OutOfBounds> {
                let bytes = match #endian {
                    1 => (input as Self::Bytes).to_be(),
                    2 => (input as Self::Bytes).to_le(),
                    _ => input as Self::Bytes,
                };
                return ::core::result::Result::Ok(bytes);
            }

            #[inline]
            fn from_bytes(bytes: Self::Bytes) -> ::core::result::Result<Self::InOut, ::modular_bitfield::error::InvalidBitPattern<Self::Bytes>> {
                let bytes = match #endian {
                    1 => Self::Bytes::from_be(bytes),
                    2 => Self::Bytes::from_le(bytes),
                    _ => bytes,
                };

                match bytes {
                    #( #from_bytes_arms ),*
                    invalid_bytes => {
                        ::core::result::Result::Err(
                            <::modular_bitfield::error::InvalidBitPattern<Self::Bytes>>::new(invalid_bytes)
                        )
                    }
                }
            }
        }
    ))
}
