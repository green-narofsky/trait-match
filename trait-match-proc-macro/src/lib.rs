//! Proc macro helper for `type-match` crate.
use ::quote::{quote, TokenStreamExt};
#[allow(unused_imports)]
use ::syn::{
    export::ToTokens,
    parse_macro_input,
    punctuated::Punctuated,
    token::{Brace, Paren},
    Attribute, AttributeArgs, Expr, Ident, ItemTrait, NestedMeta, Pat, Path, PathSegment, Token,
    TraitBound, TraitBoundModifier, TraitItem, TraitItemType, Type, TypeParamBound, TraitItemMethod,
    parse_quote,
};
use proc_macro::TokenStream;

/// Mangle a name in a deterministic way that hopefully won't
/// collide with any user code. Probably won't.
fn mangle_name(name: &Ident, descriptor: &str) -> String {
    format!("{}___sealed_trait_{}", name.to_token_stream(), descriptor)
}

/// Configuration from custom attributes on the trait item.
struct SealConfig {
    // Really, this should also store an ident.
    enum_name: Option<String>,
    /// Whether or not to add an `upcast` method to the trait.
    // If we happen to get this name in a way that lets us
    // keep a span around, we should do that.
    // So, we store an Ident.
    upcast: Option<Ident>,
    /// Determine whether to emit the enum at all.
    // If the enum key is given, or if upcast is set,
    // this should be true.
    // Otherwise, this should be false.
    make_enum: bool,
}
impl SealConfig {
    fn new() -> Self {
        Self {
            enum_name: None,
            upcast: None,
            make_enum: false,
        }
    }
    fn rename_enum(&mut self, name: String) {
        self.enum_name = Some(name);
        self.make_enum = true;
    }
    fn set_upcast(&mut self, upcast: Ident) {
        self.upcast = Some(upcast);
        // Design question: Should `upcast` imply `enum`?
        self.make_enum = true;
    }
    fn get_enum_name(&self, trait_name: &Ident) -> String {
        match self.enum_name {
            Some(ref name) => name.clone(),
            None => mangle_name(trait_name, "enum"),
        }
    }
    fn get_seal_name(&self, trait_name: &Ident) -> String {
        mangle_name(trait_name, "seal")
    }
}

#[derive(Debug)]
enum AttrArg {
    Enum(Option<Ident>),
    Upcast(Ident),
}
impl syn::parse::Parse for AttrArg {
    fn parse(input: ::syn::parse::ParseStream) -> ::syn::parse::Result<Self> {
        if input.peek(Token![enum]) {
            let _enum_token = input.parse::<Token![enum]>()?;
            if input.peek(Token![,]) || input.is_empty() {
                Ok(AttrArg::Enum(None))
            } else {
                let _equal_sign = input.parse::<Token![=]>()?;
                let name = input.parse::<Ident>()?;
                Ok(AttrArg::Enum(Some(name)))
            }
        } else if input.peek(Ident) {
            let ident = input.parse::<Ident>().unwrap();
            if format!("{}", ident) == "upcast" {
                if input.peek(Token![,]) || input.is_empty() {
                    Ok(AttrArg::Upcast(ident))
                } else {
                    let _equal_sign = input.parse::<Token![=]>()?;
                    let name = input.parse::<Ident>()?;
                    Ok(AttrArg::Upcast(name))
                }
            } else {
                Err(::syn::Error::new(ident.span(), "expected `enum` or `upcast`"))
            }
        } else {
            Err(::syn::Error::new(input.span(), "expected `enum` or `upcast`"))
        }
    }
}

/// Parse out and strip custom attributes.
fn take_attr_args(attrs: &mut Vec<Attribute>) -> ::syn::parse::Result<SealConfig> {
    let mut config = SealConfig::new();

    let mut remove_idxs = Vec::new();
    for (idx, attr) in attrs.iter().enumerate() {
        match attr.path.segments.first() {
            Some(x) if format!("{}", x.ident) == "seal" => {
                remove_idxs.push(idx);
                let parsed_args = attr.parse_args_with(Punctuated::<AttrArg, Token![,]>::parse_terminated)?;
                for x in parsed_args.iter() {
                    match x {
                        AttrArg::Enum(Some(name)) => config.rename_enum(name.to_string()),
                        AttrArg::Enum(None) => config.make_enum = true,
                        AttrArg::Upcast(name) => config.set_upcast(name.clone()),
                    }
                }
            }
            _ => (),
        }
    }

    // We don't want to reorder the attributes on this trait,
    // because there might be other proc macro attributes doing a similar thing.
    remove_idxs.into_iter().for_each(|idx| {
        attrs.remove(idx);
    });

    Ok(config)
}

/// Mark trait as sealed so you can match over it.
#[proc_macro_attribute]
pub fn sealed(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let mut input = parse_macro_input!(input as ItemTrait);
    let config = match take_attr_args(&mut input.attrs) {
        Ok(x) => x,
        Err(e) => return e.to_compile_error().into(),
    };

    let enum_vis = input.vis.clone();
    let trait_enum_name = Ident::new(&config.get_enum_name(&input.ident), input.ident.span());
    let trait_seal_name = Ident::new(&config.get_seal_name(&input.ident), input.ident.span());
    input.supertraits.push(TypeParamBound::Trait(TraitBound {
        paren_token: None,
        modifier: TraitBoundModifier::None,
        lifetimes: None,
        path: ::syn::parse_str(&format!("{}::Sealed", trait_seal_name)).unwrap(),
    }));
    let mut variants = ::proc_macro2::TokenStream::new();
    let mut sealed_impls = ::proc_macro2::TokenStream::new();
    let mut variant_from_impls = ::proc_macro2::TokenStream::new();
    for meta in args.iter() {
        variants.append_all(quote! { #meta(#meta), });
        sealed_impls.append_all(quote! { impl #trait_seal_name::Sealed for #meta {} });
        if config.make_enum {
            variant_from_impls.append_all(quote! {
                impl ::core::convert::From<#meta> for #trait_enum_name {
                    fn from(v: #meta) -> Self {
                        Self::#meta(v)
                    }
                }
            });
        }
    }

    let (upcast_trait, upcast_impls) = match config.upcast {
        Some(upcast_ident) => {
            let upcast_path: Path = ::syn::parse_str(&format!("{}::Upcast", trait_seal_name)).unwrap();
            input.supertraits.push(TypeParamBound::Trait(TraitBound {
                paren_token: None,
                modifier: TraitBoundModifier::None,
                lifetimes: None,
                path: upcast_path.clone(),
            }));
            input.items.push(TraitItem::Method(parse_quote! {
                fn #upcast_ident(self) -> #trait_enum_name
                    where Self: ::core::marker::Sized,
                {
                    <Self as #upcast_path>::upcast(self)
                }
            }));
            (
                quote! {
                    pub trait Upcast {
                        fn upcast(this: Self) -> super::#trait_enum_name;
                        // TODO: provide these two methods as well.
                        // Such will require duplicating the trait enum to
                        // contain references instead, I think?
                        // fn upcast_mut_ref(&mut self) -> &mut super::#trait_enum_name;
                        // fn upcast_ref(&self) -> &super::#trait_enum_name;
                    }
                },
                {
                    let mut impls = ::proc_macro2::TokenStream::new();
                    for meta in args.iter() {
                        impls.append_all(quote! {
                            impl #trait_seal_name::Upcast for #meta {
                                fn upcast(this: Self) -> #trait_enum_name {
                                    #trait_enum_name::#meta(this)
                                }
                            }
                        });
                    }
                    impls
                },
            )
        }
        None => (
            ::proc_macro2::TokenStream::new(),
            ::proc_macro2::TokenStream::new(),
        ),
    };

    let trait_enum = if config.make_enum {
        quote! {
            // This enum is unreachable from outside via the Upcast trait,
            // since the Upcast trait is unreachable from outside.
            // This enum is marked the same visibility as the
            // trait that it *is* reachable through, so it will
            // always be appropriately reachable.
            // So, the `private_in_public` warning is unnecessary.
            #[allow(private_in_public)]
            #[allow(non_camel_case_types)]
            #enum_vis enum #trait_enum_name {
                #variants
            }
        }
    } else {
        ::proc_macro2::TokenStream::new()
    };

    let out = input.into_token_stream();
    let result = (quote! {
        #[allow(non_snake_case)]
        mod #trait_seal_name {
            pub trait Sealed {}
            #upcast_trait
        }
        // To avoid needing to fiddle with the module system,
        // we put the Sealed impls in the same scope as the
        // the sealed trait declaration.
        #sealed_impls
        // Emit the trait declaration as is.
        #out
        // Generate an enum for matching over it!
        #trait_enum
        // Generate From impls for the enum, so we can
        // convert all implementors to the enum.
        #variant_from_impls
        #upcast_impls
    })
        .into();
    // println!("{}", result);
    result
}

/// A type that implements the trait we're doing all this for.
struct ImplType;
/// Module full of items we don't want to directly expose.
///
/// Note that this does not need to contain impls of
/// hidden traits, since the actual privacy of those traits
/// is unchanged by the arrangement of their impls.
///
/// This is just a pathing distinction, so impl blocks should be
/// placed where it is most convenient to name the implementing types.
struct HiddenItems {
    module_name: Ident,
    seal: SealedTrait,
    upcast: Option<UpcastTrait>,
}
impl ToTokens for HiddenItems {
    fn to_tokens(&self, stream: &mut ::proc_macro2::TokenStream) {
        let module_name = &self.module_name;
        let seal = &self.seal;
        let upcast = &self.upcast;
        stream.append_all(quote! {
            mod #module_name {
                #seal
                #upcast
            }
        })
    }
}
struct SealedTrait;
impl ToTokens for SealedTrait {
    fn to_tokens(&self, stream: &mut ::proc_macro2::TokenStream) {
        stream.append_all(quote! {
            pub trait Sealed {}
        });
    }
}
// TODO: upcast_ref and upcast_ref_mut
struct UpcastTrait {
    // TODO: make this a type instead
    // we need TargetEnum<Self> for this.
    target: Ident,
    /// Default implementation of the upcast method.
    default: Option<Path>,
}
impl ToTokens for UpcastTrait {
    fn to_tokens(&self, stream: &mut ::proc_macro2::TokenStream) {
        let target = &self.target;
        match self.default {
            Some(ref path) => {
                stream.append_all(quote! {
                    pub trait Upcast {
                        fn upcast(self) -> #target {
                            #path(self)
                        }
                    }
                })
            },
            None => {
                stream.append_all(quote! {
                    pub trait Upcast {
                        fn upcast(self) -> #target;
                    }
                })
            }
        }
    }
}
/// Group of items we do want to directly expose.
/// This does not place its items inside of a module.
///
/// Note that this does not, and is not intended to,
/// control visibility modifiers.
struct NakedItems {
    seal_impls: SealedTraitImpls,
    upcast_impls: UpcastImpls,
}
struct TraitEnum {
    variants: Vec<ImplType>,
}
struct TraitEnumFromVariantImpls {}
struct SealedTraitImpls {
    impls: Vec<ImplType>,
}
struct UpcastImpls {
    impls: Vec<ImplType>,
}

#[proc_macro_attribute]
pub fn unsealed(args: TokenStream, input: TokenStream) -> TokenStream {
    input
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
