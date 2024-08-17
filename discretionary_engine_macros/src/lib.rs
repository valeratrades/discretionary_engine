#![allow(clippy::get_first)]
#![allow(clippy::len_zero)]
#![allow(clippy::tabs_in_doc_comments)]
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse_macro_input;

/// Derivation of a Wrapper that allows for dynamic configuration of Protocol's params.
#[proc_macro_derive(ProtocolWrapper)]
pub fn derive_protocol_wrapper(input: TokenStream) -> TokenStream {
	let ast = parse_macro_input!(input as syn::DeriveInput);
	let name = &ast.ident;
	let _fields = if let syn::Data::Struct(syn::DataStruct {
		fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
		..
	}) = ast.data
	{
		named
	} else {
		unimplemented!()
	};

	let wrapper_name = format_ident!("{}Wrapper", name);

	let expanded = quote! {
		#[derive(Clone, Debug, Default)]
		pub struct #wrapper_name(std::cell::RefCell<#name>);
		impl std::str::FromStr for #wrapper_name {
			type Err = anyhow::Error;

			fn from_str(spec: &str) -> anyhow::Result<Self> {
				let params = #name::from_str(spec)?;
				Ok(Self(std::cell::RefCell::new(params)))
			}
		}
	};
	expanded.into()
}
